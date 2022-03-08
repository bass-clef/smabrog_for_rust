
use i18n_embed::{
    fluent::{
        fluent_language_loader,
        FluentLanguageLoader
    },
    LanguageLoader,
    unic_langid::LanguageIdentifier,
};
use mongodb::{
    *,
    bson::{
        self,
        Document
    },
    options::{
        ClientOptions,
        FindOptions,
        UpdateModifications,
    }
};
use rust_embed::RustEmbed;
use serde::{
    Deserialize,
    Serialize,
};
use std::collections::HashMap;
use std::io::BufReader;

use crate::data::SmashbrosData;


// #[cfg(dependencies = "iced")]
// pub mod iced_resource;
// use iced_resource::SmashbrosResource;

// #[cfg(dependencies = "eframe")]
pub mod eframe_resource;
pub use eframe_resource::{
    GUI_CONFIG,
    SMASHBROS_RESOURCE,
    SmashbrosResource,
};


/// i18n
#[derive(RustEmbed)]
#[folder = "locales/"]
pub struct Localizations;
/// シングルトンで i18n を保持するため
pub struct WrappedFluentLanguageLoader {
    lang: Option<FluentLanguageLoader>,
}
impl WrappedFluentLanguageLoader {
    pub fn get(&mut self) -> &FluentLanguageLoader {
        if self.lang.is_none() {
            let loader = fluent_language_loader!();
            
            match loader.load_fallback_language(&Localizations) {
                Ok(()) => self.lang = Some(loader),
                Err(_e) => {
                    self.lang = Some(FluentLanguageLoader::new(
                        "ja-JP",
                        LanguageIdentifier::from_bytes("ja-JP".as_bytes()).expect("lang parsing failed"),
                    ));
                },
            }
        }
        self.lang.as_mut().unwrap()
    }

    // 言語の変更
    pub fn change(&mut self, lang: LanguageIdentifier) {
        let loader = fluent_language_loader!();
        let _result = i18n_embed::select(&loader, &Localizations, &[lang]);

        self.lang = Some(loader);
    }
}
static mut _LANG_LOADER: WrappedFluentLanguageLoader = WrappedFluentLanguageLoader {
    lang: None,
};
#[allow(non_snake_case)]
pub fn LANG_LOADER() -> &'static mut WrappedFluentLanguageLoader { unsafe { &mut _LANG_LOADER } }
pub fn is_lang_english() -> bool { LANG_LOADER().get().current_language().language.as_str() == "en" }


/// スマブラ情報が入ったリソースファイル(serde_jsonで読み込むためのコンテナ)
#[derive(Debug, Deserialize, Serialize)]
pub struct SmashbrosResourceText {
    version: String,
    character_list: HashMap<String, String>,
    icon_list: HashMap<String, String>,

    #[serde(default)]
    i18n_convert_list: HashMap<String, String>,
    #[serde(default)]
    bgm_list: HashMap<String, bool>,
}
impl Default for SmashbrosResourceText {
    fn default() -> Self {
        Self::new()
    }
}
impl SmashbrosResourceText {
    const FILE_PATH: &'static str = "smashbros_resource.yml";
    fn new() -> Self {
        let lang = LANG_LOADER().get().current_language().language.clone();
        let path = format!("{}_{}", lang.as_str(), SmashbrosResourceText::FILE_PATH);
        let mut own = Self::load_resources(&path);
        log::info!("loaded SmashBros by {} resource version [{}.*.*]", lang.as_str(), own.version);

        // icon_list, bgm_list は全言語のを読み込んでおく
        for lang in LANG_LOADER().get().available_languages(&Localizations).unwrap() {
            let path = format!("{}_{}", lang.language.as_str(), SmashbrosResourceText::FILE_PATH);

            own.icon_list.extend(Self::load_resources(&path).icon_list);
            own.bgm_list.extend(Self::load_resources(&path).bgm_list);
        }

        own
    }

    fn load_resources(path: &str) -> Self {
        let file = std::fs::File::open(&path).unwrap();
        let reader = BufReader::new(file);
        
        match serde_yaml::from_reader::<_, Self>(reader) {
            Ok(config) => {
                config
            },
            Err(_) => {
                log::error!("invalid smashbros_resource.");
                panic!("invalid smashbros_resource.");
            }
        }
    }
}


/// 戦歴を管理するクラス
pub struct BattleHistory {
    db_client: Client,
}
impl Default for BattleHistory {
    fn default() -> Self { Self::new() }
}
impl AsMut<BattleHistory> for BattleHistory {
    fn as_mut(&mut self) -> &mut Self { self }
}
impl AsRef<BattleHistory> for BattleHistory {
    fn as_ref(&self) -> &Self { self }
}
impl BattleHistory {
    pub fn new() -> Self {
        Self {
            db_client: Self::get_client(),
        }
    }

    // DB への接続のための Client を返す
    fn get_client() -> Client {
        let mut options = async_std::task::block_on(async move {
            ClientOptions::parse(r"mongodb://localhost:27017/").await.unwrap()
        });
        options.retry_reads = Some(false);

        Client::with_options(options).expect("Failed connecting to MongoDB")
    }

    /// コレクションから検索して返す
    pub fn find_data(&mut self, filter: Option<Document>, find_options: FindOptions) -> Option<Vec<SmashbrosData>> {
        let database = self.db_client.database("smabrog-db");
        let collection_ref = database.collection("battle_data_col").clone();

        // mongodb のポインタ的なものをもらう
        let mut cursor = match async_std::task::block_on(async {
            async_std::future::timeout(
                std::time::Duration::from_secs(5),
                collection_ref.find(filter.clone(), find_options.clone())
            ).await
        }) {
            Ok(cursor) => cursor.ok().unwrap(),
            Err(_e) => {    // async_std::future::TimeoutError( _private: () )
                log::error!("find timeout. please restart smabrog.");
                return None;
            },
        };

        // ポインタ的 から ドキュメントを取得して、コンテナに格納されたのを積む
        use async_std::prelude::*;
        let mut data_list: Vec<SmashbrosData> = Vec::new();
        while let Some(document) = async_std::task::block_on(async{ cursor.next().await }) {
            let data: SmashbrosData = bson::from_bson(bson::Bson::Document(document.unwrap())).unwrap();
            data_list.push(data);
        }
        
        Some(data_list)
    }

    /// battle_data コレクションへ戦歴情報を挿入
    pub fn insert_data(&mut self, data: &SmashbrosData) -> Option<String> {
        let database = self.db_client.database("smabrog-db");
        let collection_ref = database.collection("battle_data_col").clone();
        let serialized_data = bson::to_bson(data).unwrap();
        let data_document = serialized_data.as_document().unwrap();

        // mongodb のポインタ的なものをもらう
        let result = match async_std::task::block_on(async {
            async_std::future::timeout(
                std::time::Duration::from_secs(5),
                collection_ref.insert_one(data_document.to_owned(), None)
            ).await
        }) {
            Ok(result) => result.ok().unwrap(),
            Err(_e) => {    // async_std::future::TimeoutError( _private: () )
                log::error!("insert timeout. please restart smabrog.");
                return None;
            },
        };
        
        // 何故か ObjectId が再帰的に格納されている
        Some(result.inserted_id.as_object_id().unwrap().to_hex())
    }

    /// コレクションへの戦歴情報を更新
    pub fn update_data(&mut self, data: &SmashbrosData) -> Option<String> {
        use mongodb::bson::doc;
        use crate::data::SmashbrosDataTrait;
        let id = match data.get_id() {
            Some(id) => id,
            None => {
                log::error!("[update err] failed update_data. id is None.");
                return None;
            },
        };

        let database = self.db_client.database("smabrog-db");
        let collection_ref = database.collection("battle_data_col").clone();
        let serialized_data = bson::to_bson(data).unwrap();
        let data_document = serialized_data.as_document().unwrap();

        match async_std::task::block_on(async {
            async_std::future::timeout(
                std::time::Duration::from_secs(5),
                collection_ref.update_one(
                    doc!{ "_id": mongodb::bson::oid::ObjectId::with_string(&id).unwrap() },
                    UpdateModifications::Document(doc! { "$set": data_document.to_owned() }),
                    None
                )
            ).await
        }) {
            Ok(result) => {
                if 1 == result.as_ref().ok().unwrap().modified_count {
                    return Some(id);
                }
                log::error!("[update err] failed update data {:?}.\ndata: [{:?}]", result, data);
            },
            Err(_e) => {    // async_std::future::TimeoutError( _private: () )
                log::error!("update timeout. please restart smabrog.");
                return None;
            },
        }

        return None;
    }

    /// コレクションから戦歴情報を削除
    pub fn delete_data(&mut self, data: &SmashbrosData) -> anyhow::Result<()> {
        use mongodb::bson::doc;
        use crate::data::SmashbrosDataTrait;
        let id = match data.get_id() {
            Some(id) => id,
            None => {
                log::error!("[delete err] failed delete_data. id is None.");
                return Err(anyhow::anyhow!("failed delete_data. id is None."));
            },
        };

        let database = self.db_client.database("smabrog-db");
        let collection_ref = database.collection("battle_data_col").clone();

        match async_std::task::block_on(async {
            async_std::future::timeout(
                std::time::Duration::from_secs(5),
                collection_ref.delete_one(
                    doc!{ "_id": mongodb::bson::oid::ObjectId::with_string(&id).unwrap() },
                    None
                )
            ).await
        }) {
            Ok(result) => {
                if 1 == result.as_ref().ok().unwrap().deleted_count {
                    return Ok(());
                }
                log::error!("[delete err] failed delete data {:?}.\ndata: [{:?}]", result, data);
            },
            Err(_e) => {    // async_std::future::TimeoutError( _private: () )
                log::error!("delete timeout. please restart smabrog.");
                return Err(anyhow::anyhow!("delete timeout. please restart smabrog."));
            },
        }

        Err(anyhow::anyhow!("failed delete data."))
    }

    /// battle_data コレクションから戦歴情報を 直近 result_max 件 取得
    pub fn find_data_limit(&mut self, result_max: i64) -> Option<Vec<SmashbrosData>> {
        self.find_data(
            None,
            FindOptions::builder()
                .sort(mongodb::bson::doc! { "_id": -1 })
                .limit(result_max)
                .build()
        )
    }

    /// 特定のキャラクターの戦歴を直近 limit 件取得
    pub fn find_data_by_chara_list(&mut self, character_list: Vec<String>, limit: i64, use_in: bool) -> Option<Vec<SmashbrosData>> {
        use mongodb::bson::doc;
        let filter = if use_in {
            doc! { "chara_list": {"$in": character_list } }
        } else {
            doc! { "chara_list": character_list }
        };

        self.find_data(
            Some(filter),
            FindOptions::builder()
                .sort(doc! { "_id": -1 })
                .limit(limit)
                .build()
        )
    }
}
/// シングルトンでDBを保持するため
pub struct WrappedBattleHistory {
    battle_history: Option<BattleHistory>,
}
impl WrappedBattleHistory {
    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    pub fn get(&mut self) -> &BattleHistory {
        if self.battle_history.is_none() {
            self.battle_history = Some(BattleHistory::new());
        }
        self.battle_history.as_ref().unwrap()
    }

    // mut 版
    pub fn get_mut(&mut self) -> &mut BattleHistory {
        if self.battle_history.is_none() {
            self.battle_history = Some(BattleHistory::new());
        }
        self.battle_history.as_mut().unwrap()
    }
}
static mut _BATTLE_HISTORY: WrappedBattleHistory = WrappedBattleHistory {
    battle_history: None,
};
#[allow(non_snake_case)]
pub fn BATTLE_HISTORY() -> &'static mut WrappedBattleHistory {
    unsafe { &mut _BATTLE_HISTORY }
}


pub enum SoundType {
    Bgm, File, Beep,
}
use rodio::{
    OutputStream,
    Sink,
    Source,
};
/// BGM の再生とフォルダを管理するクラス
pub struct SoundManager {
    bgm_list: Vec<String>,
    current_bgm_index: usize,
    current_file_index: usize,
    current_beep_index: usize,
    sinks_list: Vec<(Sink, OutputStream)>,
    volume: f32,
}
impl Default for SoundManager {
    fn default() -> Self { Self::new() }
}
impl SoundManager {
    pub fn new() -> Self {
        Self {
            bgm_list: Vec::new(),
            current_bgm_index: 0,
            current_file_index: 0,
            current_beep_index: 0,
            sinks_list: Vec::new(),
            volume: 1.0,
        }
    }

    fn play_source<I: rodio::Sample + Send, S: Source<Item = I> + Send + 'static>(&mut self, source: S) -> usize {
        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        let sinks = Sink::try_new(&stream_handle).unwrap();
        sinks.set_volume(self.volume);
        sinks.append(source);
        
        self.sinks_list.push((sinks, stream));

        self.sinks_list.len() - 1
    }

    // path を精査して、BGM リストを作成する
    pub fn load(&mut self, path: String) -> Result<(), anyhow::Error> {
        let path = std::path::PathBuf::from(path);
        let dir = match path.read_dir() {
            Ok(dir) => dir,
            Err(_e) => {
                log::error!("Failed read_dir. path: {:?}", path);
                return Err(anyhow::anyhow!("Failed read_dir. path: {:?}", path));
            },
        };

        self.bgm_list.clear();
        for entry in dir {
            if let Ok(entry) = entry {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    match ext.to_str().unwrap() {
                        "aac"|"alac"|"flac"|"mkv"|"mp3"|"mp4"|"ogg"|"vorbis"|"wav"|"webm" => (),
                        _ => {
                            log::warn!("Invalid extension. path: {:?}", path);
                            continue;
                        },
                    }
                } else {
                    log::warn!("Invalid extension. path: {:?}", path);
                    continue;
                }
                let file_path = path.to_string_lossy().to_string();
                self.bgm_list.push(file_path);
            }
        }

        Ok(())
    }

    /// 現在再生中のリストを破棄する
    pub fn stop(&mut self, sound_type: Option<SoundType>) {
        match sound_type {
            Some(SoundType::Bgm) => {
                let sinks = self.sinks_list.remove(self.current_bgm_index);
                sinks.0.stop();
                self.current_bgm_index = 0;
            },
            Some(SoundType::File) => {
                let sinks = self.sinks_list.remove(self.current_file_index);
                sinks.0.stop();
                self.current_file_index = 0;
            },
            Some(SoundType::Beep) => {
                let sinks = self.sinks_list.remove(self.current_beep_index);
                sinks.0.stop();
                self.current_beep_index = 0;
            },
            None => {
                while let Some(sinks) = self.sinks_list.pop() {
                    sinks.0.stop();
                }
                self.current_bgm_index = 0;
                self.current_file_index = 0;
                self.current_beep_index = 0;
            },
        }
    }

    /// ファイルから再生する (make_id が false だと SoundType::File になる)
    pub fn play_file(&mut self, file_path: String, make_id: bool) -> Option<usize> {
        if !make_id && self.is_playing(Some(SoundType::File)) {
            self.stop(Some(SoundType::File));
        }
        let path = std::path::PathBuf::from(file_path);
        let source = rodio::Decoder::new(std::fs::File::open(path).unwrap()).unwrap();

        if make_id {
            Some(self.play_source(source))
        } else {
            self.current_file_index = self.play_source(source);

            None
        }
    }

    /// BGM リストの index を再生する
    pub fn play_bgm(&mut self, index: usize) {
        if self.is_playing(Some(SoundType::Bgm)) {
            self.stop(Some(SoundType::Bgm));
        }
        log::info!("play_bgm: {:?}", &self.bgm_list[index]);
        self.current_bgm_index = self.play_file(self.bgm_list[index].clone(), true).unwrap();
    }

    /// ビープ音を鳴らす
    pub fn beep(&mut self, freq: f32, duration: std::time::Duration) {
        if self.is_playing(Some(SoundType::Beep)) {
            self.stop(Some(SoundType::Beep));
            self.current_file_index = 0;
        }
        let source = rodio::source::SineWave::new(freq).take_duration(duration);
        self.current_beep_index = self.play_source(source);
    }

    /// BGM リストの一曲をランダムで再生する
    pub fn play_bgm_random(&mut self) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        self.play_bgm(rng.gen_range( 0..self.bgm_list.len() ));
    }

    /// 音量の変更
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        for sinks in self.sinks_list.iter_mut() {
            sinks.0.set_volume(self.volume);
        }
    }

    /// 再生中かどうか
    pub fn is_playing(&self, sound_type: Option<SoundType>) -> bool {
        match sound_type {
            Some(SoundType::Bgm) => if let Some(sinks) = self.sinks_list.get(self.current_bgm_index) {
                return !sinks.0.empty();
            },
            Some(SoundType::File) => if let Some(sinks) = self.sinks_list.get(self.current_file_index) {
                return !sinks.0.empty();
            },
            Some(SoundType::Beep) => if let Some(sinks) = self.sinks_list.get(self.current_beep_index) {
                return !sinks.0.empty();
            },
            None => for sinks in self.sinks_list.iter() {
                if !sinks.0.empty() {
                    return true;
                }
            },
        }

        false
    }
}
/// シングルトンで SoundManager を保持するため
pub struct WrappedSoundManager {
    sound_manager: Option<SoundManager>,
}
impl WrappedSoundManager {
    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    pub fn get(&mut self) -> &SoundManager {
        if self.sound_manager.is_none() {
            self.sound_manager = Some(SoundManager::default());
        }
        self.sound_manager.as_ref().unwrap()
    }

    // mut 版
    pub fn get_mut(&mut self) -> &mut SoundManager {
        if self.sound_manager.is_none() {
            self.sound_manager = Some(SoundManager::default());
        }
        self.sound_manager.as_mut().unwrap()
    }
}
static mut _SOUND_MANAGER: WrappedSoundManager = WrappedSoundManager {
    sound_manager: None,
};
#[allow(non_snake_case)]
pub fn SOUND_MANAGER() -> &'static mut WrappedSoundManager {
    unsafe { &mut _SOUND_MANAGER }
}

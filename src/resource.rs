
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
    gui_config,
    smashbros_resource,
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

            loader.load_fallback_language(&Localizations)
                .expect("Error while loading fallback language");
        
            self.lang = Some(loader);
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
static mut LANG: WrappedFluentLanguageLoader = WrappedFluentLanguageLoader {
    lang: None,
};
pub fn lang_loader() -> &'static mut WrappedFluentLanguageLoader { unsafe { &mut LANG } }
pub fn is_lang_english() -> bool { lang_loader().get().current_language().language.as_str() == "en" }


/// スマブラ情報が入ったリソースファイル(serde_jsonで読み込むためのコンテナ)
#[derive(Debug, Deserialize, Serialize)]
pub struct SmashbrosResourceText {
    version: String,
    character_list: HashMap<String, String>,
    icon_list: HashMap<String, String>,

    #[serde(default)]
    i18n_convert_list: HashMap<String, String>,
}
impl Default for SmashbrosResourceText {
    fn default() -> Self {
        Self::new()
    }
}
impl SmashbrosResourceText {
    const FILE_PATH: &'static str = "smashbros_resource.json";
    fn new() -> Self {
        let lang = lang_loader().get().current_language().language.clone();
        let path = format!("{}_{}", lang.as_str(), SmashbrosResourceText::FILE_PATH);
        let mut own = Self::load_resources(&path);
        log::info!("loaded SmashBros by {} resource version [{}.*.*]", lang.as_str(), own.version);

        // icon_list は全言語のを読み込んでおく
        for lang in lang_loader().get().available_languages(&Localizations).unwrap() {
            let path = format!("{}_{}", lang.language.as_str(), SmashbrosResourceText::FILE_PATH);

            own.icon_list.extend(Self::load_resources(&path).icon_list);
        }

        own
    }

    fn load_resources(path: &str) -> Self {
        let file = std::fs::File::open(&path).unwrap();
        let reader = BufReader::new(file);
        
        match serde_json::from_reader::<_, Self>(reader) {
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
static mut BATTLE_HISTORY: WrappedBattleHistory = WrappedBattleHistory {
    battle_history: None,
};
pub fn battle_history() -> &'static mut WrappedBattleHistory {
    unsafe { &mut BATTLE_HISTORY }
}


use i18n_embed::{
    fluent::{
        fluent_language_loader,
        FluentLanguageLoader
    },
    LanguageLoader,
    unic_langid::LanguageIdentifier,
};
use mongodb::*;
use mongodb::options::{
    ClientOptions,
    FindOptions,
};
use rust_embed::RustEmbed;
use serde::{
    Deserialize,
    Serialize,
};
use std::collections::HashMap;
use std::io::BufReader;

use crate::resource::bson::Document;
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
        // MongoDBへの接続(の代わりに作成)とdatabaseの取得
        let options = async_std::task::block_on(async move {
            ClientOptions::parse("mongodb://localhost:27017/").await.unwrap()
        });

        let client = Client::with_options(options).unwrap();

        Self {
            db_client: client,
        }
    }

    /// なにかのクエリを非同期でタイムアウト付きで実行する
    pub fn do_query_with_timeout<F, T>(future: F) -> Option<T>
    where
        F: async_std::future::Future<Output = Result<T, mongodb::error::Error>>, 
    {
        async_std::task::block_on(async {
            let timeout = async_std::future::timeout(std::time::Duration::from_secs(5), future).await;

            match timeout {
                Ok(result) => match result {
                    Ok(result_object) => Some(result_object),
                    Err(e) => {
                        // mongodb::error
                        log::error!("[db_err] {:?}", e);

                        None
                    },
                },
                Err(e) => {
                    // async_std::timeout::error
                    log::error!("[timeout] {:?}", e);

                    None
                }
            }
        })
    }

    
    // コレクションから検索して返す
    pub fn find_data(&self, filter: Option<Document>, find_options: FindOptions) -> Option<Vec<SmashbrosData>> {
        let database = self.db_client.database("smabrog-db");
        let collection_ref = database.collection("battle_data_col").clone();

        // mongodb のポインタ的なものをもらう
        let mut cursor: Cursor = match async_std::task::block_on(async {
            let timeout = async_std::future::timeout(std::time::Duration::from_secs(5), 
                collection_ref.find(filter, find_options)
            ).await;

            match timeout {
                Ok(cursor) => match cursor {
                    Ok(cursor) => Ok(cursor),
                    Err(e) => Err(anyhow::anyhow!( format!("{:?}", e) )),   // mongodb::error -> anyhow
                },
                Err(e) => Err(anyhow::anyhow!( format!("{:?}", e) ))    // async_std::timeout::error -> anyhow
            }
        }) {
            Ok(cursor) => cursor,
            Err(e) => {
                log::error!("[err] {:?}", e);
                return None
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

        match Self::do_query_with_timeout(
            collection_ref.insert_one( data_document.to_owned(), None )
        ) {
            // 何故か ObjectId が再帰的に格納されている
            Some(result) => Some(result.inserted_id.as_object_id().unwrap().to_hex()),
            None => return None,
        }        
    }

    /// battle_data コレクションから戦歴情報を 直近10件 取得
    pub fn find_data_limit_10(&self) -> Option<Vec<SmashbrosData>> {
        self.find_data(
            None,
            FindOptions::builder()
                .sort(crate::resource::bson::doc! { "_id": -1 })
                .limit(10)
                .build()
        )
    }

    /// 特定のキャラクターの戦歴を取得
    pub fn find_data_by_chara_list(&self, character_list: Vec<String> ) -> Option<Vec<SmashbrosData>> {
        self.find_data(
            Some(crate::resource::bson::doc! { "chara_list": character_list }),
            FindOptions::builder()
                .sort(crate::resource::bson::doc! { "_id": -1 })
                .limit(1000)    // 念の為とこれ以上とっても結果が変わらなそう
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


use mongodb::*;
use mongodb::options::{
    ClientOptions,
    FindOptions,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::collections::HashMap;
use std::io::BufReader;

use crate::resource::bson::*;
use crate::data::SmashbrosData;


// #[cfg(dependencies = "iced")]
// pub mod iced_resource;
// use iced_resource::SmashbrosResource;

// #[cfg(dependencies = "eframe")]
pub mod eframe_resource;
pub use eframe_resource::{
    GUI_CONFIG,
    SMASHBROS_RESOURCE,
};


/// スマブラ情報が入ったリソースファイル(serde_jsonで読み込むためのコンテナ)
#[derive(Serialize, Deserialize, Debug)]
pub struct SmashbrosResourceText {
    version: String,
    character_list: HashMap<String, String>,
    icon_list: HashMap<String, String>,
}
impl Default for SmashbrosResourceText {
    fn default() -> Self {
        Self::new()
    }
}
impl SmashbrosResourceText {
    const FILE_PATH: &'static str = "smashbros_resource.json";
    fn new() -> Self {
        let file = std::fs::File::open(SmashbrosResourceText::FILE_PATH).unwrap();
        let reader = BufReader::new(file);
        
        match serde_json::from_reader::<_, Self>(reader) {
            Ok(config) => {
                log::info!("loaded SmashBros resource version [{}.*.*]", config.version);
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
                .sort(doc! { "_id": -1 })
                .limit(10)
                .build()
        )
    }

    /// 特定のキャラクターの戦歴を取得
    pub fn find_data_by_chara_list(&self, character_list: Vec<String> ) -> Option<Vec<SmashbrosData>> {
        self.find_data(
            Some(doc! { "chara_list": character_list }),
            FindOptions::builder()
                .sort(doc! { "_id": -1 })
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
pub static mut BATTLE_HISTORY: WrappedBattleHistory = WrappedBattleHistory {
    battle_history: None,
};

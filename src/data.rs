
use chrono::DateTime;
use difflib::sequencematcher::SequenceMatcher;
use mongodb::*;
use mongodb::options::{
    ClientOptions,
    FindOptions,
};
use serde::{
    de::Visitor,
    Deserialize,
    Deserializer,
    ser::SerializeStruct,
    Serialize,
    Serializer,
};
use std::collections::HashMap;
use std::io::{
    BufReader,
};

use crate::data::bson::*;

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

/// 画像とかも入ったリソース
pub struct SmashbrosResource {
    pub version: String,
    pub character_list: HashMap<String, String>,
    pub icon_list: HashMap<String, iced_winit::image::Handle>,
    pub order_image_list: HashMap<i32, iced_winit::image::Handle>,
}
impl SmashbrosResource {
    fn new() -> Self {
        let text = SmashbrosResourceText::new();
        let mut icon_list: HashMap<String, iced_winit::image::Handle> = HashMap::new();
        for (character_name, file_name) in text.icon_list.iter() {
            icon_list.insert(
                character_name.to_string(),
                iced_winit::image::Handle::from_path(format!( "icon/{}", file_name ))
            );
            
        }
        let mut order_image_list: HashMap<i32, iced_winit::image::Handle> = HashMap::new();
        for order in 0..4 {
            order_image_list.insert(
                order,
                iced_winit::image::Handle::from_path(format!( "resource/result_player_order_{}_color.png", order ))
            );
        }

        Self {
            version: text.version,
            character_list: text.character_list,
            icon_list: icon_list,
            order_image_list: order_image_list,
        }
    }

    pub fn get_image_handle(&self, character_name: String) -> Option<iced_winit::image::Handle> {
        if !self.icon_list.contains_key(&character_name) {
            return None;
        }

        Some(self.icon_list[&character_name].clone())
    }

    pub fn get_order_handle(&self, order: i32) -> Option<iced_winit::image::Handle> {
        if order <= 0 || 5 <= order {
            return None;
        }

        Some(self.order_image_list[&order].clone())
    }
}

/// シングルトンでリソースを保持するため
pub struct WrappedSmashbrosResource {
    smashbros_resource: Option<SmashbrosResource>
}
impl WrappedSmashbrosResource {
    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    pub fn get(&mut self) -> &SmashbrosResource {
        if self.smashbros_resource.is_none() {
            self.smashbros_resource = Some(SmashbrosResource::new());
        }
        self.smashbros_resource.as_ref().unwrap()
    }
}
pub static mut SMASHBROS_RESOURCE: WrappedSmashbrosResource = WrappedSmashbrosResource {
    smashbros_resource: None,
};


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
                        log::info!("[db_err] {:?}", e);

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
        let database = self.db_client.database("smabrog-db");
        let collection_ref = database.collection("battle_data_col").clone();

        /* mongodb のポインタ的なものをもらう */
        let mut cursor: Cursor = match async_std::task::block_on(async {
            let timeout = async_std::future::timeout(std::time::Duration::from_secs(5), 
                collection_ref.find(
                    None,
                    FindOptions::builder()
                        .sort(doc! { "_id": -1 })
                        .limit(10)
                        .build()
                )
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
        // */

        /* mongodb のポインタ的なものをもらう *
        // 上記コードを下記に変更すると STATUS_STACK_OVERFLOW で落ちる(やってる内容は同じだし謎のバグ)
        // async_std::task::block_on を読んだ時点で起こるっぽい(insert_with_2 は普通に実行できるから更に謎)
        // アタッチデバッグしたら async-executor という crate の run あたりでスタックが溢れてるっぽい
        let mut cursor: Cursor = match Self::do_query_with_timeout(
            collection_ref.find(
                None,
                FindOptions::builder()
                    .sort(doc! { "_id": -1 })
                    .limit(10)
                    .build()
            )
        ) {
            Some(cursor) => cursor,
            None => return None,
        };
        // */
        
        // ポインタ的 から ドキュメントを取得して、コンテナに格納されたのを積む
        use async_std::prelude::*;
        let mut data_list: Vec<SmashbrosData> = Vec::new();
        while let Some(document) = async_std::task::block_on(async{ cursor.next().await }) {
            let data: SmashbrosData = bson::from_bson(bson::Bson::Document(document.unwrap())).unwrap();
            data_list.push(data);
        }
        
        Some(data_list)
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


/// 値を推測して一番高いものを保持しておくためのクラス
#[derive(Clone, Debug)]
pub struct ValueGuesser<K: Clone + Eq + std::hash::Hash> {
    value_count_list: HashMap<K, i32>,
    max_value: K,
    max_count: i32,
    max_border: i32,
}
impl<K: std::hash::Hash + Clone + Eq> ValueGuesser<K> {
    /// @param value 初期値
    pub fn new(value: K) -> Self {
        Self {
            value_count_list: HashMap::new(),
            max_value: value.clone(),
            max_count: 0,
            max_border: 5,
        }
    }

    /// 試行回数の設定 初期値:5
    pub fn set_border(mut self, max_border: i32) -> Self {
        self.max_border = max_border;

        self
    }

    /// 値が決定したかどうか
    pub fn is_decided(&self) -> bool {
        self.max_border <= self.max_count
    }

    /// 一番出現回数が高い Value を返す
    /// using clone.
    pub fn get(&self) -> K {
        self.max_value.clone()
    }

    /// 値を強制する
    /// using clone.
    pub fn set(&mut self, value: K) {
        self.max_value = value.clone();
        self.max_count = self.max_border;
        *self.value_count_list.entry(value).or_insert(0) = self.max_border;
    }

    /// 値を推測する
    /// using clone.
    pub fn guess(&mut self, value: &K) {
        if self.is_decided() {
            return;
        }

        *self.value_count_list.entry(value.clone()).or_insert(0) += 1;

        if self.max_count < self.value_count_list[value] {
            self.max_value = value.clone();
            self.max_count = self.value_count_list[value];
        }
    }
}


/// プレイヤーのグループの種類,色
/// キャラクターの色数 == グループの数 == 8
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash)]
pub enum PlayerGroup {
    Unknown, Red, Blue, Green, Yellow,
}
impl std::str::FromStr for PlayerGroup {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Unknown" => Ok(Self::Unknown),
            "Red" => Ok(Self::Red),
            "Blue" => Ok(Self::Blue),
            "Green" => Ok(Self::Green),
            "Yellow" => Ok(Self::Yellow),
            _ => Ok(Self::Unknown),
        }
    }
}
/// ルール
/// Time   : 時間制限あり[2,2:30,3], ストック数は上限なしの昇順, HPはバースト毎に0%に初期化
/// Stock  : 時間制限あり[3,4,5,6,7], ストック数は上限[1,2,3]の降順, HPはバースト毎に0%に初期化
/// Stamina: 時間制限あり[3,4,5,6,7], ストック数は上限[1,2,3]の降順, HPは上限[100,150,200,250,300]の降順
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleRule {
    Unknown, Time, Stock, Stamina, 
}
impl std::str::FromStr for BattleRule {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Unknown" => Ok(Self::Unknown),
            "Time" => Ok(Self::Time),
            "Stock" => Ok(Self::Stock),
            "Stamina" => Ok(Self::Stamina),
            _ => Ok(Self::Unknown),
        }
    }
}


/// データトレイト
pub trait SmashbrosDataTrait {
    // setter
    /// DB key
    fn get_id(&self) -> Option<String>;

    /// 最後に保存した時刻の取得
    fn get_saved_time(&self) -> Option<std::time::Instant>;

    /// 試合開始時刻の取得
    fn get_start_time(&self) -> Option<DateTime<chrono::Local>>;
    /// 試合終了時刻の取得
    fn get_end_time(&self) -> Option<DateTime<chrono::Local>>;

    /// プレイヤー数の取得
    fn get_player_count(&self) -> i32;
    /// ルールの取得
    fn get_rule(&self) -> BattleRule;

    /// 時間制限の取得
    fn get_max_time(&self) -> std::time::Duration;
    /// プレイヤーの最大ストック数の取得
    fn get_max_stock(&self, player_number: i32) -> i32;
    /// プレイヤーの最大HPの取得
    fn get_max_hp(&self, player_number: i32) -> i32;

    /// プレイヤーのキャラクターの取得
    fn get_character(&self, player_number: i32) -> String;
    /// プレイヤーのグループの取得
    fn get_group(&self, player_number: i32) -> PlayerGroup;
    /// プレイヤーのストック数の取得
    fn get_stock(&self, player_number: i32) -> i32;
    /// プレイヤーの順位の取得
    fn get_order(&self, player_number: i32) -> i32;
    /// プレイヤーの順位の取得
    fn get_power(&self, player_number: i32) -> i32;

    // gettter
    /// DB key
    fn set_id(&mut self, value: Option<String>);

    /// 最後に保存した時刻の設定
    fn set_saved_time(&mut self, value: Option<std::time::Instant>);

    /// 試合開始時刻の設定
    fn set_start_time(&mut self, value: Option<DateTime<chrono::Local>>);
    /// 試合終了時刻の設定
    fn set_end_time(&mut self, value: Option<DateTime<chrono::Local>>);

    /// プレイヤー数の設定
    fn set_player_count(&mut self, value: i32);
    /// ルールの設定
    fn set_rule(&mut self, value: BattleRule);

    /// 時間制限の設定
    fn set_max_time(&mut self, value: std::time::Duration);
    /// プレイヤーの最大ストック数の設定
    fn set_max_stock(&mut self, player_number: i32, value: i32);
    /// プレイヤーの最大HPの設定
    fn set_max_hp(&mut self, player_number: i32, value: i32);

    /// プレイヤーのキャラクターの設定
    fn set_character(&mut self, player_number: i32, value: String);
    /// プレイヤーのグループの設定
    fn set_group(&mut self, player_number: i32, value: PlayerGroup);
    /// プレイヤーのストック数の設定
    fn set_stock(&mut self, player_number: i32, value: i32);
    /// プレイヤーの順位の設定
    fn set_order(&mut self, player_number: i32, value: i32);
    /// プレイヤーの順位の設定
    fn set_power(&mut self, player_number: i32, value: i32);

    // is系
    /// 試合中かどうか
    fn is_playing_battle(&self) -> bool;
    /// 試合後かどうか
    fn is_finished_battle(&self) -> bool;

    // ルールは確定しているか
    fn is_decided_rule(&self) -> bool;

    /// 時間制限は確定しているか
    fn is_decided_max_time(&self) -> bool;
    /// プレイヤーの最大ストックは確定しているか
    fn is_decided_max_stock(&self, player_number: i32) -> bool;
    /// プレイヤーの最大HPは確定しているか
    fn is_decided_max_hp(&self, player_number: i32) -> bool;

    /// プレイヤーが使用しているキャラクターは確定しているか
    fn is_decided_character_name(&self, player_number: i32) -> bool;
    /// プレイヤーのストックは確定しているか
    fn is_decided_stock(&self, player_number: i32) -> bool;
    /// プレイヤーの順位は確定しているか
    fn is_decided_order(&self, player_number: i32) -> bool;
    /// プレイヤーの戦闘力は確定しているか
    fn is_decided_power(&self, player_number: i32) -> bool;

    // convert系
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// ドキュメント.フィールド名
#[derive(Clone, Copy, Debug)]
enum SmashbrosDataField {
    Id(&'static str),
    StartTime(&'static str), EndTime(&'static str),
    PlayerCount(&'static str), RuleName(&'static str),
    MaxTime(&'static str), MaxStockList(&'static str), MaxHpList(&'static str),
    CharaList(&'static str),
    GroupList(&'static str),
    StockList(&'static str),
    OrderList(&'static str),
    PowerList(&'static str)
}
impl SmashbrosDataField {
    fn name(&self) -> &'static str {
        match *self {
            Self::Id(name) |
            Self::StartTime(name) | Self::EndTime(name) |
            Self::PlayerCount(name) | Self::RuleName(name) |
            Self::MaxTime(name) | Self::MaxStockList(name) | Self::MaxHpList(name) |
            Self::CharaList(name) |
            Self::GroupList(name) |
            Self::StockList(name) |
            Self::OrderList(name) |
            Self::PowerList(name) => {
                name
            },
        }
    }
}
impl<'de> Deserialize<'de> for SmashbrosDataField {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
        where D: Deserializer<'de>
    {
        deserializer.deserialize_identifier(SmashbrosDataFieldVisitor)
    }
}
/// DB コンテナ(フィールド名用)
#[derive(Debug)]
struct SmashbrosDataFieldVisitor;
impl<'de> Visitor<'de> for SmashbrosDataFieldVisitor {
    type Value = SmashbrosDataField;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        formatter.write_str("expected field name for SmashbrosData.")
    }

    fn visit_str<E>(self, maybe_field_name: &str) -> std::result::Result<Self::Value, E>
        where E: serde::de::Error,
    {
        for &field in &SmashbrosData::FIELDS {
            if field.name() == maybe_field_name {
                return Ok(field)
            }
        }

        Err(serde::de::Error::unknown_field(maybe_field_name, SmashbrosData::FIELD_NAMES))
    }
}
/// DB コンテナ(データ用)
struct SmashbrosDataVisitor;
impl<'de> Visitor<'de> for SmashbrosDataVisitor {
    type Value = SmashbrosData;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        formatter.write_str("expected data SmashbrosData.")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, <A as serde::de::MapAccess<'de>>::Error>
        where A: serde::de::MapAccess<'de>,
    {
        use std::str::FromStr;

        let mut data = SmashbrosData::default();
        data.initialize_data();

        // ?: データ値が格納されてないのと、DBにフィールドがそもそもなかったのとは別なので、一度開いて Some で閉じる
        while let Some(key) = map.next_key::<SmashbrosDataField>()? {
            match key {
                // ID
                SmashbrosDataField::Id(_) => {
                    let value: HashMap<String, String> = map.next_value::<HashMap<String, String>>()?;
                    data.set_id(Some( value["$oid"].clone() ));
                },

                SmashbrosDataField::StartTime(_) => {
                    data.set_start_time(Some( DateTime::<chrono::Local>::from_str(&map.next_value::<String>()?).unwrap() ));
                },
                SmashbrosDataField::EndTime(_) => {
                    data.set_end_time(Some( DateTime::<chrono::Local>::from_str(&map.next_value::<String>()?).unwrap() ));
                },

                SmashbrosDataField::PlayerCount(_) => {
                    data.initialize_battle( map.next_value::<i32>()? );
                    data.set_saved_time(Some( std::time::Instant::now() ));
                },
                SmashbrosDataField::RuleName(_) => {
                    data.set_rule(BattleRule::from_str( &map.next_value::<String>()? ).unwrap());
                },

                SmashbrosDataField::MaxTime(_) => {
                    use std::ops::Add;
                    let times = map.next_value::<Vec<i32>>()?;
                    let time = std::time::Duration::from_secs((times[0] * 60) as u64)
                        .add(std::time::Duration::from_secs(times[1] as u64));
                    data.set_max_time(time);
                },
                SmashbrosDataField::MaxStockList(_) => {
                    for (player_number, max_stock) in map.next_value::<Vec<i32>>()?.iter().enumerate() {
                        data.set_max_stock(player_number as i32, *max_stock);
                    }
                },
                SmashbrosDataField::MaxHpList(_) => {
                    for (player_number, max_hp) in map.next_value::<Vec<i32>>()?.iter().enumerate() {
                        data.set_max_hp(player_number as i32, *max_hp);
                    }
                },

                SmashbrosDataField::CharaList(_) => {
                    for (player_number, chara_name) in map.next_value::<Vec<String>>()?.iter().enumerate() {
                        data.set_character(player_number as i32, chara_name.clone());
                    }
                },
                SmashbrosDataField::GroupList(_) => {
                    for (player_number, group_name) in map.next_value::<Vec<String>>()?.iter().enumerate() {
                        data.set_group(player_number as i32, PlayerGroup::from_str( &group_name ).unwrap());
                    }
                },
                SmashbrosDataField::StockList(_) => {
                    for (player_number, stock) in map.next_value::<Vec<i32>>()?.iter().enumerate() {
                        data.set_stock(player_number as i32, *stock);
                    }
                },
                SmashbrosDataField::OrderList(_) => {
                    for (player_number, order) in map.next_value::<Vec<i32>>()?.iter().enumerate() {
                        data.set_order(player_number as i32, *order);
                    }
                },
                SmashbrosDataField::PowerList(_) => {
                    for (player_number, power) in map.next_value::<Vec<i32>>()?.iter().enumerate() {
                        data.set_power(player_number as i32, *power);
                    }
                }
            }
        }

        Ok(data)
    }
}

/// データ (engine 処理用)
#[derive(Debug, Clone)]
pub struct SmashbrosData {
    /// this is database key. // {db}.{col}.find(doc!{ _id: ObjectId(collection_id) });
    db_collection_id: Option<String>,
    saved_time: Option<std::time::Instant>,
    
    // 基本データ
    start_time: Option<DateTime<chrono::Local>>,
    end_time: Option<DateTime<chrono::Local>>,
    
    player_count: i32,
    rule_name: BattleRule,

    // ルール条件
    max_time: Option<std::time::Duration>,
    max_stock_list: Option<Vec<ValueGuesser<i32>>>,
    max_hp_list: Option<Vec<ValueGuesser<i32>>>,

    // プレイヤーの数だけ存在するデータ
    chara_list: Vec<ValueGuesser<String>>,
    group_list: Vec<ValueGuesser<PlayerGroup>>,
    stock_list: Vec<ValueGuesser<i32>>,
    order_list: Vec<ValueGuesser<i32>>,
    power_list: Vec<ValueGuesser<i32>>,
}
impl Default for SmashbrosData {
    fn default() -> Self { Self::new() }
}
impl Drop for SmashbrosData {
    fn drop(&mut self) {
        self.finalize_battle();
    }
}
impl AsRef<SmashbrosData> for SmashbrosData {
    fn as_ref(&self) -> &SmashbrosData { self }
}
impl AsMut<SmashbrosData> for SmashbrosData {
    fn as_mut(&mut self) -> &mut SmashbrosData { self }
}
impl SmashbrosDataTrait for SmashbrosData {
    // getter
    fn get_id(&self) -> Option<String> { self.db_collection_id.clone() }
    fn get_saved_time(&self) -> Option<std::time::Instant> { self.saved_time.clone() }

    fn get_start_time(&self) -> Option<DateTime<chrono::Local>> { self.start_time.clone() }
    fn get_end_time(&self) -> Option<DateTime<chrono::Local>> { self.end_time.clone() }

    fn get_player_count(&self) -> i32 { self.player_count }
    fn get_rule(&self) -> BattleRule { self.rule_name.clone() }

    fn get_max_time(&self) -> std::time::Duration {
        self.max_time.unwrap_or(std::time::Duration::from_secs(0))
    }
    fn get_max_stock(&self, player_number: i32) -> i32 {
        let max_stock_list = match &self.max_stock_list {
            None => return -1,
            Some(max_stock_list) => max_stock_list,
        };
        if max_stock_list.len() <= player_number as usize {
            return -1;
        }

        max_stock_list[player_number as usize].get()
    }
    fn get_max_hp(&self, player_number: i32) -> i32 {
        let max_hp_list = match &self.max_hp_list {
            None => return -1,
            Some(max_hp_list) => max_hp_list,
        };
        if max_hp_list.len() <= player_number as usize {
            return -1;
        }

        max_hp_list[player_number as usize].get()
    }

    fn get_character(&self, player_number: i32) -> String {
        if self.chara_list.len() <= player_number as usize {
            return Self::CHARACTER_NAME_UNKNOWN.to_string();
        }

        self.chara_list[player_number as usize].get()
    }
    fn get_group(&self, player_number: i32) -> PlayerGroup {
        if self.group_list.len() <= player_number as usize {
            return PlayerGroup::Unknown;
        }

        self.group_list[player_number as usize].get()
    }
    fn get_stock(&self, player_number: i32) -> i32 {
        if self.stock_list.len() <= player_number as usize {
            return -1;
        }

        self.stock_list[player_number as usize].get()
    }
    fn get_order(&self, player_number: i32) -> i32 {
        if self.order_list.len() <= player_number as usize {
            return -1;
        }

        self.order_list[player_number as usize].get()
    }
    fn get_power(&self, player_number: i32) -> i32 {
        if self.power_list.len() <= player_number as usize {
            return -1;
        }

        self.power_list[player_number as usize].get()
    }

    // setter
    fn set_id(&mut self, value: Option<String>) { self.db_collection_id = value; }
    fn set_saved_time(&mut self, value: Option<std::time::Instant>) { self.saved_time = value; }

    fn set_start_time(&mut self, value: Option<DateTime<chrono::Local>>) { self.start_time = value; }
    fn set_end_time(&mut self, value: Option<DateTime<chrono::Local>>) { self.end_time = value; }

    fn set_player_count(&mut self, value: i32) { self.player_count = value; }
    fn set_rule(&mut self, value: BattleRule) { self.rule_name = value; }

    fn set_max_time(&mut self, value: std::time::Duration) { self.max_time = Some(value) }
    fn set_max_stock(&mut self, player_number: i32, value: i32) { (*self.max_stock_list.as_mut().unwrap())[player_number as usize].set(value); }
    fn set_max_hp(&mut self, player_number: i32, value: i32) { (*self.max_hp_list.as_mut().unwrap())[player_number as usize].set(value); }

    fn set_character(&mut self, player_number: i32, value: String) { self.chara_list[player_number as usize].set(value); }
    fn set_group(&mut self, player_number: i32, value: PlayerGroup) { self.group_list[player_number as usize].set(value); }
    fn set_stock(&mut self, player_number: i32, value: i32) { self.stock_list[player_number as usize].set(value); }
    fn set_order(&mut self, player_number: i32, value: i32) { self.order_list[player_number as usize].set(value); }
    fn set_power(&mut self, player_number: i32, value: i32) { self.power_list[player_number as usize].set(value); }

    // is_{hoge}
    fn is_playing_battle(&self) -> bool {
        if let Some(end_time) = self.end_time {
            if end_time <= chrono::Local::now() {
                // もし終わりが記録されていて、now が end_time よりも後なら、試合後。
                return false;
            }
        }

        let start_time = match self.start_time {
            Some(start_time) => start_time,
            // None の場合は initialize_battle が呼ばれていないため、試合前。
            None => return false,
        };

        // 実質 (start_time <= now < end_time)
        start_time <= chrono::Local::now()
    }
    fn is_finished_battle(&self) -> bool {
        if self.start_time.is_none() {
            return false;
        }

        let end_time = match self.end_time {
            Some(end_time) => end_time,
            None => return false,
        };

        end_time <= chrono::Local::now()
    }

    fn is_decided_rule(&self) -> bool {
        self.rule_name != BattleRule::Unknown
    }

    fn is_decided_max_time(&self) -> bool {
        self.max_time.is_some()
    }
    fn is_decided_max_stock(&self, player_number: i32) -> bool {
        self.max_stock_list.is_some() && -1 != self.as_ref().get_max_stock(player_number)
    }
    fn is_decided_max_hp(&self, player_number: i32) -> bool {
        self.max_hp_list.is_some() && -1 != self.as_ref().get_max_hp(player_number)
    }

    fn is_decided_character_name(&self, player_number: i32) -> bool {
        // 名前の一致度が 100% ならそれ以上は変更し得ない
        !self.chara_list.is_empty() && self.chara_list[player_number as usize].is_decided()
    }
    fn is_decided_stock(&self, player_number: i32) -> bool {
        // ストック数が 1 の時はそれ以上減ることは仕様上ないはずなので skip
        !self.stock_list.is_empty() && 1 == self.stock_list[player_number as usize].get()
    }
    fn is_decided_order(&self, player_number: i32) -> bool {
        !self.order_list.is_empty() && self.order_list[player_number as usize].is_decided()
    }
    fn is_decided_power(&self, player_number: i32) -> bool {
        !self.power_list.is_empty() && self.power_list[player_number as usize].is_decided()
    }

    // as系
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
impl Serialize for SmashbrosData {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
        where S: Serializer,
    {
        let mut state = serializer.serialize_struct("SmashbrosData", Self::FIELDS.len())?;
        state.serialize_field( "start_time", &format!( "{:?}", self.get_start_time().unwrap_or(chrono::Local::now())) )?;
        state.serialize_field( "end_time", &format!( "{:?}", self.get_end_time().unwrap_or(chrono::Local::now()) ) )?;

        state.serialize_field( "player_count", &self.get_player_count() )?;
        state.serialize_field( "rule_name", &format!("{:?}", self.get_rule()) )?;

        let times = self.get_max_time();
        state.serialize_field( "max_time", &vec![times.as_secs() as i32 / 60, times.as_secs() as i32 % 60] )?;
        state.serialize_field( "max_stock_list",
            &self.max_stock_list.as_ref().unwrap_or(&vec![ValueGuesser::new(-1)])
                .iter().map(|value| value.get() ).collect::<Vec<i32>>()
        )?;
        state.serialize_field( "max_hp_list",
            &self.max_hp_list.as_ref().unwrap_or(&vec![ValueGuesser::new(-1)])
                .iter().map(|value| value.get() ).collect::<Vec<i32>>()
        )?;

        state.serialize_field( "chara_list", &self.chara_list.iter().map(|value| value.get().to_string() ).collect::<Vec<String>>() )?;
        state.serialize_field( "group_list", &self.group_list.iter().map(|value| format!("{:?}", value.get()) ).collect::<Vec<String>>() )?;
        state.serialize_field( "stock_list", &self.stock_list.iter().map(|value| value.get() ).collect::<Vec<i32>>() )?;
        state.serialize_field( "order_list", &self.order_list.iter().map(|value| value.get() ).collect::<Vec<i32>>() )?;
        state.serialize_field( "power_list", &self.power_list.iter().map(|value| value.get() ).collect::<Vec<i32>>() )?;

        state.end()
    }
}
impl<'de> Deserialize<'de> for SmashbrosData {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
        where D: Deserializer<'de>
    {
        deserializer.deserialize_struct("SmashbrosData", Self::FIELD_NAMES, SmashbrosDataVisitor)
    }
}
impl SmashbrosData {
    // db にあるフィールド名
    const FIELD_NAMES: &'static [&'static str] = &[
        "_id",
        "start_time", "end_time",
        "player_count",
        "rule_name",
        "max_time", "max_stock_list", "max_hp_list",
        "chara_list",
        "group_list",
        "stock_list",
        "order_list",
        "power_list"
    ];
    // db に突っ込むときのフィールド名
    const FIELDS: [SmashbrosDataField; 13] = [
        SmashbrosDataField::Id{ 0:"_id" },
        SmashbrosDataField::StartTime{ 0:"start_time" }, SmashbrosDataField::EndTime{ 0:"end_time" },
        SmashbrosDataField::PlayerCount{ 0: "player_count" },
        SmashbrosDataField::RuleName{ 0: "rule_name" },
        SmashbrosDataField::MaxTime{ 0: "max_time" }, SmashbrosDataField::MaxStockList{ 0: "max_stock_list" }, SmashbrosDataField::MaxHpList{ 0: "max_hp_list" },
        SmashbrosDataField::CharaList{ 0: "chara_list" },
        SmashbrosDataField::GroupList{ 0: "group_list" },
        SmashbrosDataField::StockList{ 0: "stock_list" },
        SmashbrosDataField::OrderList{ 0: "order_list" },
        SmashbrosDataField::PowerList{ 0: "power_list" }
    ];
    // キャラクター名が不明時の文字列
    const CHARACTER_NAME_UNKNOWN: &'static str = "unknown";

    fn new() -> Self {
        Self {
            db_collection_id: None,
            saved_time: None,

            start_time: None,
            end_time: None,

            player_count: 0,
            rule_name: BattleRule::Unknown,

            max_time: None,
            max_stock_list: None,
            max_hp_list: None,

            chara_list: vec![ValueGuesser::new("".to_string())],
            group_list: vec![ValueGuesser::new(PlayerGroup::Unknown)],
            stock_list: vec![ValueGuesser::new(-1)],
            order_list: vec![ValueGuesser::new(-1)],
            power_list: vec![ValueGuesser::new(-1)],
        }
    }

    /// データの初期化
    /// @return bool false:初期化せず true:初期化済み
    fn initialize_data(&mut self) -> bool {
        if SmashbrosData::CHARACTER_NAME_UNKNOWN == self.chara_list[0].get() {
            // 初期値代入してあったら処理しない(全部同時にされるので chara_list だけで比較)
            return false;
        }

        // 削除
        self.start_time = None;
        self.end_time = None;
        
        self.rule_name = BattleRule::Unknown;
        self.max_time = None;

        self.chara_list.clear();
        self.group_list.clear();
        self.stock_list.clear();
        self.order_list.clear();
        self.power_list.clear();

        // 削除も非同期に要素が参照されうるので確保だけは適当にしとく
        let mut max_stock_list: Vec<ValueGuesser<i32>> = Vec::new();
        let mut max_hp_list: Vec<ValueGuesser<i32>> = Vec::new();
        for _ in 0..8 {
            max_stock_list.push( ValueGuesser::new(-1) );
            max_hp_list.push( ValueGuesser::new(-1) );

            self.chara_list.push( ValueGuesser::new("".to_string()) );
            self.group_list.push( ValueGuesser::new(PlayerGroup::Unknown) );
            self.stock_list.push( ValueGuesser::new(-1) );
            self.order_list.push( ValueGuesser::new(-1) );
            self.power_list.push( ValueGuesser::new(-1) );
        }
        self.max_stock_list = Some( max_stock_list );
        self.max_hp_list = Some( max_hp_list );

        return true;
    }

    /// 試合前の処理
    pub fn initialize_battle(&mut self, player_count: i32) {
        if !self.finalize_battle() {
            // 初期値代入済みだとしない
            return;
        }

        self.db_collection_id = None;
        self.player_count = player_count;

        // 初期値代入
        let mut max_stock_list: Vec<ValueGuesser<i32>> = Vec::new();
        let mut max_hp_list: Vec<ValueGuesser<i32>> = Vec::new();
        self.chara_list.clear();
        self.group_list.clear();
        self.stock_list.clear();
        self.order_list.clear();
        self.power_list.clear();
        for _ in 0..self.player_count {
            max_stock_list.push( ValueGuesser::new(-1) );
            max_hp_list.push( ValueGuesser::new(-1) );

            self.chara_list.push( ValueGuesser::new(SmashbrosData::CHARACTER_NAME_UNKNOWN.to_string()) );
            self.group_list.push( ValueGuesser::new(PlayerGroup::Unknown) );
            self.stock_list.push( ValueGuesser::new(-1).set_border(2) );
            self.order_list.push( ValueGuesser::new(-1) );
            self.power_list.push( ValueGuesser::new(-1) );
        }
        self.max_stock_list = Some( max_stock_list );
        self.max_hp_list = Some( max_hp_list );

        match player_count {
            2 => {
                // 1 on 1 の時はチームカラーが固定
                self.group_list[0].set(PlayerGroup::Red);
                self.group_list[1].set(PlayerGroup::Blue);
            },
            _ => ()
        };
    }
    /// 試合後の処理
    /// @return bool true:ついでにデータも削除した
    pub fn finalize_battle(&mut self) -> bool {
        // 試合が終わっていると保存する
        if self.is_finished_battle() {
            self.save_battle();
        }

        self.initialize_data()
    }
    /// 試合開始の設定 (ReadyToFight,Match 系で呼ぶ)
    pub fn start_battle(&mut self) {
        self.start_time = Some(chrono::Local::now());
    }
    /// 試合終了の設定 (GameEnd,Result 系で呼ぶ)
    pub fn finish_battle(&mut self) {
        self.end_time = Some(chrono::Local::now());
    }
    /// 試合情報の保存
    pub fn save_battle(&mut self) {
        if self.db_collection_id.is_some() {
            // 既に保存済み
            return;
        }

        self.db_collection_id = match self.player_count {
            2 => unsafe{BATTLE_HISTORY.get_mut()}.insert_data(self),
            _ => None,
        };

        self.saved_time = Some(std::time::Instant::now());
    }

    /// 制限時間の推測
    pub fn guess_max_time(&mut self, maybe_time: u64) {
        if self.is_decided_max_time() {
            return;
        }

        // ルールによる制限
        match self.get_rule() {
            BattleRule::Time => {
                if maybe_time < 2*60 || 3*60 < maybe_time {
                    return;
                }
            },
            BattleRule::Stock | BattleRule::Stamina => {
                if maybe_time < 3*60 || 7*60 < maybe_time {
                    return;
                }
            },
            _ => (),
        }

        self.set_max_time(std::time::Duration::from_secs(maybe_time));

        log::info!("max time {:?}s", maybe_time);
    }

    /// 最大ストック数の推測
    pub fn guess_max_stock(&mut self, player_number: i32, maybe_stock: i32) {
        if self.is_decided_max_stock(player_number) {
            return;
        }

        // ルールによる制限
        match self.get_rule() {
            BattleRule::Stock | BattleRule::Stamina => {
                if maybe_stock < 1 || 3 < maybe_stock {
                    return;
                }
            },
            _ => (),
        }

        (*self.max_stock_list.as_mut().unwrap())[player_number as usize].guess(&maybe_stock);

        log::info!("max stock {}p: {}? => {:?}", player_number+1, maybe_stock, self.get_max_stock(player_number));
    }
    /// 最大ストック数は確定しているか
    pub fn all_decided_max_stock(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_max_stock(player_number) )
    }
    /// 最大HPの推測
    pub fn guess_max_hp(&mut self, player_number: i32, maybe_hp: i32) {
        if self.is_decided_max_stock(player_number) {
            return;
        }

        (*self.max_hp_list.as_mut().unwrap())[player_number as usize].guess(&maybe_hp);

        log::info!("max hp {}p: {}? => {:?}", player_number+1, maybe_hp, self.get_max_hp(player_number));
    }
    /// 最大HPは確定しているか
    pub fn all_decided_max_hp(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_max_hp(player_number) )
    }

    /// プレイヤーが使用しているキャラクターの設定
    pub fn guess_character_name(&mut self, player_number: i32, maybe_character_name: String) {
        if self.is_decided_character_name(player_number) {
            // 一致度が 100% だと比較しない
            return;
        }
        
        if unsafe{SMASHBROS_RESOURCE.get()}.character_list.contains_key(&maybe_character_name) {
            // O(1)
            self.set_character(player_number, maybe_character_name.clone());
        } else {
            // リソースから一番一致率が高い名前を設定する
            let mut max_ratio = 0.0;
            let mut matcher = SequenceMatcher::new("", "");
            let mut chara_name = Self::CHARACTER_NAME_UNKNOWN;
            for (character_name, _) in unsafe{SMASHBROS_RESOURCE.get()}.character_list.iter() {
                matcher.set_seqs(character_name, &maybe_character_name);
                if max_ratio < matcher.ratio() {
                    max_ratio = matcher.ratio();
                    chara_name = &character_name;
                    if 1.0 <= max_ratio {
                        break;
                    }
                }
            }
            if chara_name != Self::CHARACTER_NAME_UNKNOWN {
                self.chara_list[player_number as usize].guess(&chara_name.to_string());
            }
        }

        log::info!("chara {}p: \"{}\"? => {:?}", player_number+1, maybe_character_name, self.get_character(player_number));
    }
    /// 全員分が使用しているキャラクターは確定しているか
    pub fn all_decided_character_name(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_character_name(player_number) )
    }

    /// プレイヤーのストックの推測
    pub fn guess_stock(&mut self, player_number: i32, maybe_stock: i32) {
        if self.stock_list[player_number as usize].is_decided()
            && self.stock_list[player_number as usize].get() == maybe_stock
        {
            return;
        }

        // ルールによる制限
        match self.get_rule() {
            BattleRule::Stock | BattleRule::Stamina => {
                if maybe_stock < 1 || 3 < maybe_stock {
                    return;
                }
            },
            _ => (),
        }

        self.stock_list[player_number as usize].guess(&maybe_stock);
        if self.stock_list[player_number as usize].is_decided() {
            self.stock_list[player_number as usize].set(maybe_stock)
        }

        log::info!("stock {}p: {}? => {:?}", player_number+1, maybe_stock, self.get_stock(player_number));
    }
    /// 全員分のストックは確定しているか
    pub fn all_decided_stock(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_stock(player_number) )
    }

    /// プレイヤーの順位の推測
    pub fn guess_order(&mut self, player_number: i32, maybe_order: i32) {
        if self.is_decided_order(player_number) {
            return;
        }

        self.order_list[player_number as usize].guess(&maybe_order);

        if self.order_list[player_number as usize].is_decided() {
            // 信頼性が高い順位は他のユーザーの順位をも確定させる
            match self.player_count {
                2 => {
                    let other_player_number = self.player_count-1 - player_number;
                    let other_maybe_order = self.player_count - (maybe_order-1);
                    self.order_list[other_player_number as usize].set(other_maybe_order);
                    log::info!( "order {}p: {}? => {:?}", other_player_number+1, other_maybe_order, self.get_order(other_player_number) );
                },
                _ => ()
            };
        }

        // 全員分の順位が確定していると最後に倒されたプレイヤーのストックを減らす
        if self.all_decided_order() {
            match self.player_count {
                2 => {
                    // 最下位は 0 にする
                    let min_order = self.order_list.iter().map(|stock| stock.get()).max().unwrap();
                    let (min_order_player_number, _) = self.order_list.iter().enumerate().filter(|(_, order)| order.get() == min_order).last().unwrap();
                    self.stock_list[min_order_player_number].set(0);
                },
                _ => ()
            };
        }

        log::info!( "order {}p: {}? => {:?}", player_number+1, maybe_order, self.get_order(player_number));
    }
    /// 全員分の順位は確定しているか
    pub fn all_decided_order(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_order(player_number) )
    }

    /// プレイヤーの戦闘力の推測 (一桁以下は無視)
    pub fn guess_power(&mut self, player_number: i32, maybe_power: i32) {
        if self.is_decided_power(player_number) || maybe_power < 100 {
            return;
        }

        self.power_list[player_number as usize].guess(&maybe_power);

        log::info!("power {}p: {}? => {:?}", player_number+1, maybe_power, self.get_power(player_number));
    }
    /// 全員分の戦闘力は確定しているか
    pub fn all_decided_power(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_power(player_number) )
    }

    /// プレイヤーの結果は取得できているか
    pub fn all_decided_result(&self) -> bool {
        self.all_decided_power() && self.all_decided_order()
    }
}

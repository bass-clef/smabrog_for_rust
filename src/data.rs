
use chrono::DateTime;
use difflib::sequencematcher::SequenceMatcher;
use mongodb::*;
use mongodb::options::{
    ClientOptions,
    FindOptions
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
                println!("loaded SmashBros resource version [{}.*.*]", config.version);
                config
            },
            Err(_) => {
                panic!("invalid smashbros_resource.");
            }
        }
    }
}

// 画像とかも入ったリソース
pub struct SmashbrosResource {
    pub version: String,
    pub character_list: HashMap<String, String>,
    pub icon_list: HashMap<String, iced_winit::image::Handle>,
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

        Self {
            version: text.version,
            character_list: text.character_list,
            icon_list: icon_list,
        }
    }

    pub fn get_image_handle(&self, character_name: String) -> Option<iced_winit::image::Handle> {
        if !self.icon_list.contains_key(&character_name) {
            return None;
        }

        Some(self.icon_list[&character_name].clone())
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


/* 戦歴を管理するクラス */
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

    /// battle_data コレクションへ戦歴情報を挿入
    // [test data]
    // use smabrog-db
    // db.createCollection("battle_data_with_2_col")
    // db.battle_data_with_2_col.insert({start_time: '2021-5-10 17:10:30', end_time: '2021-5-10 17:10:30', rule: 'Stock', max_stock: 3, player: 'KIRBY', opponent: 'KIRBY', stock: 3, stock_diff: 2, order: 1, power: 8000000, power_diff: 300000})
    pub fn insert_with_2(&mut self, data: &SmashbrosData) -> Option<String> {
        let database = self.db_client.database("smabrog-db");
        let collection_ref = database.collection("battle_data_with_2_col").clone();
        let serialized_data = bson::to_bson(data).unwrap();
        let data_document = serialized_data.as_document().unwrap();

        async_std::task::block_on(async move {
            match collection_ref.insert_one(data_document.to_owned(), None).await {
                Ok(ret) => {
                    println!("[ok] finished battle. {:?}", ret);

                    // 何故か ObjectId が再帰的に格納されている
                    Some(ret.inserted_id.as_object_id().unwrap().to_hex())
                },
                Err(e) => {
                    println!("[err] {:?}", e);

                    None
                }
            }
        })
    }

    /// battle_data コレクションから戦歴情報を 直近10件 取得
    pub fn find_with_2_limit_10(&self) -> Option<Vec<SmashbrosData>> {
        let database = self.db_client.database("smabrog-db");
        let collection_ref = database.collection("battle_data_with_2_col").clone();

        // mongodb のポインタ的なものをもらう
        let mut cursor: Cursor = match async_std::task::block_on(async move {
            collection_ref.find(
                None,
                FindOptions::builder()
                    .sort(doc! { "_id": -1 })
                    .limit(10)
                    .build()
            ).await
        }) {
            Ok(cursor) => cursor,
            Err(_) => return None
        };

        // ポインタ的 から ドキュメントを取得して、コンテナに格納されたのを積む
        use async_std::stream;
        use async_std::prelude::*;
        use async_std::task::block_on;
        let mut data_list: Vec<SmashbrosData> = Vec::new();
        while let Some(document) = block_on(async{ cursor.next().await }) {
            let data: SmashbrosData = bson::from_bson(bson::Bson::Document(document.unwrap())).unwrap();
//            let data = serde_json::from_str::<SmashbrosData>(document.unwrap().as_str()).unwrap();
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


// プレイヤーのグループの種類,色
#[derive(Debug, Clone)]
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
// ルール
#[derive(Debug, Clone)]
pub enum BattleRule {
    Unknown, Time, Stock, HealthPoint,
}
impl std::str::FromStr for BattleRule {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Unknown" => Ok(Self::Unknown),
            "Time" => Ok(Self::Time),
            "Stock" => Ok(Self::Stock),
            "HealthPoint" => Ok(Self::HealthPoint),
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

    /// プレイヤーの最大ストック数の取得
    fn get_max_stock(&self, player_number: i32) -> i32;

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

    /// プレイヤーの最大ストック数の設定
    fn set_max_stock(&mut self, player_number: i32, value: i32);

    // is系
    /// 試合中かどうか
    fn is_playing_battle(&self) -> bool;
    /// 試合後かどうか
    fn is_finished_battle(&self) -> bool;

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
    Rule(&'static str),
    Player(&'static str), Opponent(&'static str),
    Stock(&'static str), StockDiff(&'static str), Order(&'static str), Power(&'static str), PowerDiff(&'static str),
    MaxStock(&'static str),
}
impl SmashbrosDataField {
    fn name(&self) -> &'static str {
        match *self {
            Self::Id(name) |
            Self::StartTime(name) | Self::EndTime(name) |
            Self::Rule(name) |
            Self::Player(name) | Self::Opponent(name) |
            Self::Stock(name) | Self::StockDiff(name) | Self::Order(name) | Self::Power(name) | Self::PowerDiff(name) |
            Self::MaxStock(name) => {
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
use serde::de;
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
        data.initialize_battle(2);
        data.set_saved_time(Some(std::time::Instant::now()));

        let mut stock_diff = 0;
        let mut power_diff = 0;

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
                SmashbrosDataField::Rule(_) => {
                    data.set_rule(BattleRule::from_str( &map.next_value::<String>()? ).unwrap())
                },

                SmashbrosDataField::Player(_) => {
                    data.set_character(0, map.next_value::<String>()?);
                },
                SmashbrosDataField::Opponent(_) => {
                    data.set_character(1, map.next_value::<String>()?);
                },
                SmashbrosDataField::Stock(_) => {
                    data.set_stock(0, map.next_value::<i32>()?);
                },
                SmashbrosDataField::StockDiff(_) => {
                    stock_diff = map.next_value::<i32>()?;
                },
                SmashbrosDataField::Order(_) => {
                    data.set_order(0, map.next_value::<i32>()?);
                },
                SmashbrosDataField::Power(_) => {
                    data.set_power(0, map.next_value::<i32>()?);
                },
                SmashbrosDataField::PowerDiff(_) => {
                    power_diff = map.next_value::<i32>()?;
                },
                SmashbrosDataField::MaxStock(_) => {
                    let max_stock = map.next_value::<i32>()?;
                    data.set_max_stock(0, max_stock);
                    data.set_max_stock(1, max_stock);
                }
            }
        }

        data.set_stock(1, data.get_stock(0) - stock_diff);
        data.set_order(1, data.get_player_count() - data.get_order(0) + 1);
        data.set_power(1, data.get_power(0) - power_diff);

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

    // プレイヤーの数だけ存在するデータ
    character_name_list: Vec<(String, f32)>,
    group_list: Vec<(PlayerGroup, f32)>,
    stock_list: Vec<(i32, f32)>,
    order_list: Vec<(i32, f32)>,
    power_list: Vec<(i32, f32)>,

    max_stock_list: Vec<i32>,
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

    fn get_character(&self, player_number: i32) -> String {
        if self.character_name_list.len() < 2 {
            return Self::CHARACTER_NAME_UNKNOWN.to_string();
        }

        self.character_name_list[player_number as usize].0.clone()
    }
    fn get_group(&self, player_number: i32) -> PlayerGroup {
        if self.character_name_list.len() < 2 {
            return PlayerGroup::Unknown;
        }

        self.group_list[player_number as usize].0.clone()
    }
    fn get_stock(&self, player_number: i32) -> i32 {
        if self.character_name_list.len() < 2 {
            return -1;
        }

        self.stock_list[player_number as usize].0
    }
    fn get_order(&self, player_number: i32) -> i32 {
        if self.character_name_list.len() < 2 {
            return -1;
        }

        self.order_list[player_number as usize].0
    }
    fn get_power(&self, player_number: i32) -> i32 {
        if self.character_name_list.len() < 2 {
            return -1;
        }

        self.power_list[player_number as usize].0
    }

    fn get_max_stock(&self, player_number: i32) -> i32 {
        if self.character_name_list.len() < 2 {
            return -1;
        }

        self.max_stock_list[player_number as usize]
    }

    // setter
    fn set_id(&mut self, value: Option<String>) { self.db_collection_id = value; }
    fn set_saved_time(&mut self, value: Option<std::time::Instant>) { self.saved_time = value; }

    fn set_start_time(&mut self, value: Option<DateTime<chrono::Local>>) { self.start_time = value; }
    fn set_end_time(&mut self, value: Option<DateTime<chrono::Local>>) { self.end_time = value; }

    fn set_player_count(&mut self, value: i32) { self.player_count = value; }
    fn set_rule(&mut self, value: BattleRule) { self.rule_name = value; }

    fn set_character(&mut self, player_number: i32, value: String) { self.character_name_list[player_number as usize].0 = value; }
    fn set_group(&mut self, player_number: i32, value: PlayerGroup) { self.group_list[player_number as usize].0 = value; }
    fn set_stock(&mut self, player_number: i32, value: i32) { self.stock_list[player_number as usize].0 = value; }
    fn set_order(&mut self, player_number: i32, value: i32) { self.order_list[player_number as usize].0 = value; }
    fn set_power(&mut self, player_number: i32, value: i32) { self.power_list[player_number as usize].0 = value; }
    fn set_max_stock(&mut self, player_number: i32, value: i32) { self.max_stock_list[player_number as usize] = value; }


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
    fn is_decided_character_name(&self, player_number: i32) -> bool {
        // 名前の一致度が 100% ならそれ以上は変更し得ない
        !self.character_name_list.is_empty() && 1.0 <= self.character_name_list[player_number as usize].1
    }
    fn is_decided_stock(&self, player_number: i32) -> bool {
        // ストック数が 1 の時はそれ以上減ることは仕様上ないはずなので skip
        !self.stock_list.is_empty() && 1 == self.stock_list[player_number as usize].0
    }
    fn is_decided_order(&self, player_number: i32) -> bool {
        !self.order_list.is_empty() && 1.0 <= self.order_list[player_number as usize].1
    }
    fn is_decided_power(&self, player_number: i32) -> bool {
        !self.power_list.is_empty() && 1.0 <= self.power_list[player_number as usize].1
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
        state.serialize_field( "rule", &format!("{:?}", self.get_rule()) )?;
        state.serialize_field( "max_stock", &self.get_max_stock(0) )?;
        state.serialize_field( "player", &self.get_character(0) )?;
        state.serialize_field( "opponent", &self.get_character(1) )?;
        state.serialize_field( "stock", &self.get_stock(0) )?;
        state.serialize_field( "stock_diff", &(self.get_stock(0) - self.get_stock(1)) )?;
        state.serialize_field( "order", &self.get_order(0) )?;
        state.serialize_field( "power", &self.get_power(0) )?;
        state.serialize_field( "power_diff", &(self.get_power(0) - self.get_power(1)) )?;

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
        "rule",
        "player", "opponent",
        "stock", "stock_diff", "order", "power", "power_diff",
        "max_stock"
    ];
    // db に突っ込むときのフィールド名
    const FIELDS: [SmashbrosDataField; 12] = [
        SmashbrosDataField::Id{ 0:"_id" },
        SmashbrosDataField::StartTime{ 0:"start_time" }, SmashbrosDataField::EndTime{ 0:"end_time" },
        SmashbrosDataField::Rule{ 0:"rule" },
        SmashbrosDataField::Player{ 0:"player" }, SmashbrosDataField::Opponent{ 0:"opponent" },
        SmashbrosDataField::Stock{ 0:"stock" }, SmashbrosDataField::StockDiff{ 0:"stock_diff" }, SmashbrosDataField::Order{ 0:"order" }, SmashbrosDataField::Power{ 0:"power" }, SmashbrosDataField::PowerDiff{ 0:"power_diff" },
        SmashbrosDataField::MaxStock{ 0:"max_stock" },
    ];
    // キャラクター名が不明時の文字列
    const CHARACTER_NAME_UNKNOWN: &'static str = "unknown";

    fn new() -> Self {
        Self {
            db_collection_id: None,
            saved_time: None,

            player_count: 0,
            rule_name: BattleRule::Unknown,
            start_time: None,
            end_time: None,

            character_name_list: vec![("".to_string(), 0.0)],
            group_list: vec![],
            stock_list: vec![],
            order_list: vec![],
            power_list: vec![],

            max_stock_list: vec![],
        }
    }

    /// データの初期化
    /// @return bool false:初期化せず true:初期化済み
    fn initialize_data(&mut self) -> bool {
        if SmashbrosData::CHARACTER_NAME_UNKNOWN == self.character_name_list[0].0 {
            // 初期値代入してあったら処理しない(全部同時にされるので character_name_list だけで比較)
            return false;
        }

        // 削除
        self.start_time = None;
        self.end_time = None;

        self.character_name_list.clear();
        self.group_list.clear();
        self.stock_list.clear();
        self.order_list.clear();
        self.power_list.clear();
        self.max_stock_list.clear();

        // 削除も非同期に要素が参照されうるので確保だけは適当にしとく
        for _ in 0..4 {
            self.character_name_list.push( ("".to_string(), 0.0 ) );
            self.group_list.push( (PlayerGroup::Unknown, 0.0) );
            self.stock_list.push( (-1, 0.0) );
            self.order_list.push( (-1, 0.0) );
            self.power_list.push( (-1, 0.0) );

            self.max_stock_list.push( -1 );
        }

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
        self.character_name_list.clear();
        self.group_list.clear();
        self.stock_list.clear();
        self.order_list.clear();
        self.power_list.clear();
        self.max_stock_list.clear();
        for player_number in 0..self.player_count {
            self.character_name_list.push( (SmashbrosData::CHARACTER_NAME_UNKNOWN.to_string(), 0.0 ) );
            self.group_list.push( (PlayerGroup::Unknown, 0.0) );
            self.stock_list.push( (-1, 0.0) );
            self.order_list.push( (-1, 0.0) );
            self.power_list.push( (-1, 0.0) );

            self.max_stock_list.push( -1 );
        }
        match player_count {
            2 => {
                // 1 on 1 の時はチームカラーが固定
                self.group_list[0] = (PlayerGroup::Red, 1.0);
                self.group_list[1] = (PlayerGroup::Blue, 1.0);
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
            2 => unsafe{BATTLE_HISTORY.get_mut()}.insert_with_2(self),
            _ => None,
        };

        self.saved_time = Some(std::time::Instant::now());
    }

    /// プレイヤーが使用しているキャラクターの設定
    pub fn guess_character_name(&mut self, player_number: i32, maybe_character_name: String) {
        if 1.0 <= self.character_name_list[player_number as usize].1 {
            // 一致度が 100% だと比較しない
            return;
        }
        
        if unsafe{SMASHBROS_RESOURCE.get()}.character_list.contains_key(&maybe_character_name) {
            // O(1)
            self.character_name_list[player_number as usize] = ( maybe_character_name.clone(), 1.0 );
        } else {
            // O(1+N)
            let mut max_ratio = self.character_name_list[player_number as usize].1;
            let mut matcher = SequenceMatcher::new("", "");
            for (character_name, _) in unsafe{SMASHBROS_RESOURCE.get()}.character_list.iter() {
                matcher.set_seqs(character_name, &maybe_character_name);
                if max_ratio < matcher.ratio() {
                    max_ratio = matcher.ratio();
                    self.character_name_list[player_number as usize] = ( character_name.clone(), max_ratio );
                    if 1.0 <= max_ratio {
                        break;
                    }
                } else if 0.87 < matcher.ratio() && character_name == &self.character_name_list[player_number as usize].0 {
                    // 同じ名前が渡された場合,僅かにそのキャラ名の一致度を上げる
                    // 5% 上げると 0.87 以上が 3 回ほどで 100% 以上になる
                    self.character_name_list[player_number as usize].1 *= 1.05;
                    break;
                }
            }
        }

        println!("chara {}p: \"{}\"? => {:?}", player_number+1, maybe_character_name, self.character_name_list[player_number as usize]);
    }
    /// 全員分が使用しているキャラクターは確定しているか
    pub fn all_decided_character_name(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_character_name(player_number) )
    }

    /// プレイヤーのストックの設定
    pub fn guess_stock(&mut self, player_number: i32, stock: i32) {
        if self.stock_list[player_number as usize].0 == stock {
            return;
        }

        if -1 == self.stock_list[player_number as usize].0 {
            // 初回代入
            self.stock_list[player_number as usize] = ( stock, 1.0 );
            if -1 == self.max_stock_list[player_number as usize] {
                self.max_stock_list[player_number as usize] = stock;
                if self.max_stock_list.iter().all(|&stock| 0 < stock) {
                    // 全員のストック数が確定すると最大値を推測する
                    if let Some(&maybe_max_stock) = self.max_stock_list.iter().max() {
                        self.max_stock_list.iter_mut().for_each(|max_stock| *max_stock = maybe_max_stock);
                        println!("rule(stock): {}", maybe_max_stock);
                    }
                }
            }
        }

        // 出鱈目な数値が来ると確定度合いを下げる (0未満 or 現在よりストックが増えた状態)
        if stock < 0 || self.stock_list[player_number as usize].0 < stock {
            // 10% 未満はもう計算しない
            if 0.1 < self.stock_list[player_number as usize].1 {
                self.stock_list[player_number as usize].1 /= 2.0;
            }
        } else if stock == self.stock_list[player_number as usize].0 - 1 {
            // ストックはデクリメントしかされない保証
            self.stock_list[player_number as usize].0 = stock;
        }

        println!("stock {}p: {}? => {:?}", player_number+1, stock, self.stock_list[player_number as usize]);
    }
    /// 全員分のストックは確定しているか
    pub fn all_decided_stock(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_stock(player_number) )
    }

    /// プレイヤーの順位の設定
    pub fn guess_order(&mut self, player_number: i32, maybe_order: i32) {
        if self.is_decided_order(player_number) {
            return;
        }
        
        // ストック数と矛盾がなければ確定
        // [プレイヤー数 - player_numberより少ないストックを持つプレイヤー数] と order を比較
        let player_stock = self.stock_list[player_number as usize].0;
        let under_order_player_count = self.player_count - self.stock_list.iter().filter(|&stocks| player_stock > stocks.0 ).count() as i32;
        if -1 == self.order_list[player_number as usize].0 {
            if maybe_order == under_order_player_count {
                self.order_list[player_number as usize] = ( maybe_order, 1.0 );
            } else {
                // [サドンデス or 同ストックのまま GameEnd]になってるか推測して、そうでなければ誤検出とする
                if 2 <= self.stock_list.iter().filter(|&stocks| player_stock == stocks.0 ).count() {
                    // サドンデスでの決着と予想して(ストック数の誤検出は考慮しない)、設定された順位を優先する
                    self.order_list[player_number as usize] = ( maybe_order, 1.0 );
                } else {
                    self.order_list[player_number as usize] = ( maybe_order, 0.1 );
                }
            }
        } else {
            if maybe_order == under_order_player_count {
                // 初回矛盾でも仏の名のもとに 3 回償えば許される()
                self.order_list[player_number as usize].1 += 0.31;
            } else {
                self.order_list[player_number as usize].1 -= 0.31;
            }
        }

        // 重複がなくてすべての順位が確定していると信用度を上げる
        let sum = self.order_list.iter().fold(0, |acc, orders| acc + orders.0);
        if sum == (1..self.player_count+1).collect::<Vec<i32>>().iter().fold(0, |acc, num| acc + num) {
            self.order_list.iter_mut().for_each(|orders| (*orders).1 = 1.1 );
        }

        if 1.0 == self.order_list[player_number as usize].1 {
            // 信頼性が高い順位は他のユーザーの順位をも確定させる
            match self.player_count {
                2 => {
                    let other_player_number = self.player_count-1 - player_number;
                    let other_maybe_order = self.player_count - (maybe_order-1);
                    self.order_list[other_player_number as usize] = ( other_maybe_order, 1.0 );
                    println!("order {}p: {}? => {:?}", other_player_number+1, other_maybe_order, self.order_list[other_player_number as usize]);
                },
                _ => ()
            };
        }

        // 全員分の順位が確定していると最後に倒されたプレイヤーのストックを減らす
        if self.all_decided_order() {
            match self.player_count {
                2 => {
                    // 最下位は 0 にする
                    let min_order = self.order_list.iter().map(|(stock, _)| stock).max().unwrap();
                    let (min_order_player_number, _) = self.order_list.iter().enumerate().filter(|(_, (order, _))| order == min_order).last().unwrap();
                    self.stock_list[min_order_player_number].0 = 0;
                },
                _ => ()
            };
        }

        println!("order {}p: {}? => {:?}", player_number+1, maybe_order, self.order_list[player_number as usize]);
    }
    /// 全員分の順位は確定しているか
    pub fn all_decided_order(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_order(player_number) )
    }

    /// プレイヤーの戦闘力の設定 (一桁以下は無視)
    pub fn guess_power(&mut self, player_number: i32, maybe_power: i32) {
        if self.is_decided_power(player_number) || maybe_power < 10 {
            return;
        }

        // 数値上昇のアニメーションの間の差分の数値も沢山送られてくる可能性があるので、
        // 同じ数値だと一致率を上げて、違う数値だと下げる事を何度もする想定
        // [初期値 or 代入されている戦闘力の 10% の誤差範囲]で変動
        let power_diff = (self.power_list[player_number as usize].0 - maybe_power).abs();
        if self.power_list[player_number as usize].0 == maybe_power && 0 <= self.power_list[player_number as usize].0 {
            // 同じ戦闘力が 5 回でほぼ確定
            self.power_list[player_number as usize].1 += 0.11;
        } else if -1 == self.power_list[player_number as usize].0 || power_diff < self.power_list[player_number as usize].0 / 10 {
            // 数値変動。一致率は変動する確率が高いので、初期値は 50%
            self.power_list[player_number as usize] = ( maybe_power, 0.5 );
        }

        println!("power {}p: {}? => {:?}", player_number+1, maybe_power, self.power_list[player_number as usize]);
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

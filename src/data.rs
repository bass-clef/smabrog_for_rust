
use difflib::sequencematcher::SequenceMatcher;
use mongodb::{
    Client, Database, options::ClientOptions, options::StreamAddress
};
use serde::{
    Deserialize, Serialize
};
use std::collections::HashMap;
use std::io::{
    BufReader
};

// プレイヤーのグループの種類,色
enum PlayerGroup {
    Unknown, Red, Blue, Green, Yellow,
}
// ルール
enum BattleRule {
    Unknown, Time, Stock, HealthPoint,
}

// スマブラ情報が入ったコンフィグデータ
static SMASHBROS_CHARACTER_LIST_FILE_NAME: &str = "smashbros_config.json";
#[derive(Serialize, Deserialize, Debug)]
pub struct SmashbrosConfig {
    version: String,
    character_list: HashMap<String, String>,
}

/// 収集したデータ郡
pub struct SmashbrosData {
    // 基本データ
    pub smashbros_config: SmashbrosConfig,
    pub start_time: std::time::Instant,
    pub end_time: std::time::Instant,
    
    player_count: i32,
    rule_name: String,

    // プレイヤーの数だけ存在するデータ
    character_name_list: Vec<(String, f32)>,
    order_list: Vec<(i32, f32)>,
    stock_list: Vec<(i32, f32)>,
    group_list: Vec<(PlayerGroup, f32)>,

    max_stock_list: Vec<i32>,
}
impl Default for SmashbrosData {
    fn default() -> Self { Self::new() }
}
impl SmashbrosData {
    fn new() -> Self {
        let file = std::fs::File::open(SMASHBROS_CHARACTER_LIST_FILE_NAME).unwrap();
        let reader = BufReader::new(file);
        
        let smashbros_config: SmashbrosConfig = match serde_json::from_reader::<_, SmashbrosConfig>(reader) {
            Ok(config) => {
                println!("loaded config version [{}.*.*]", config.version);
                config
            },
            Err(_) => {
                panic!("invalid smashbros_config.");
            }
        };

        Self {
            smashbros_config: smashbros_config,
            player_count: 0,
            rule_name: "unknown".to_string(),
            start_time: std::time::Instant::now(),
            end_time: std::time::Instant::now(),

            character_name_list: vec![],
            order_list: vec![],
            stock_list: vec![],
            group_list: vec![],

            max_stock_list: vec![],
        }
    }

    /// プレイヤーデータの初期化 (ReadyToFight 系で呼んで)
    pub fn initialize_battle(&mut self, player_count: i32) {
        if !self.character_name_list.is_empty() {
            // 初期化してあったら処理しない(全部同時に初期化されるので character_name_list だけで比較)
            return;
        }

        println!("new battle with {}", player_count);
        self.player_count = player_count;
        self.character_name_list.clear();
        self.order_list.clear();
        self.stock_list.clear();
        self.group_list.clear();
        for _ in 0..self.player_count {
            self.character_name_list.push( ("unknown".to_string(), 0.0 ) );
            self.order_list.push( (-1, 0.0) );
            self.stock_list.push( (-1, 0.0) );
            self.group_list.push( (PlayerGroup::Unknown, 0.0) );

            self.max_stock_list.push( -1 );
        }
    }

    pub fn get_player_count(&self) -> i32 { self.player_count }

    /// プレイヤーが使用しているキャラクターの設定
    pub fn set_character_name(&mut self, player_number: i32, maybe_character_name: String) {
        if 1.0 <= self.character_name_list[player_number as usize].1 {
            // 一致度が 100% だと比較しない
            return;
        }
        
        if self.smashbros_config.character_list.contains_key(&maybe_character_name) {
            // O(1)
            self.character_name_list[player_number as usize] = ( maybe_character_name.clone(), 1.0 );
        } else {
            // O(1+N)
            let mut max_ratio = 0.0;
            let mut matcher = SequenceMatcher::new("", "");
            for (character_name, character_jpn) in self.smashbros_config.character_list.iter() {
                matcher.set_seqs(character_name, &maybe_character_name);
                if max_ratio < matcher.ratio() {
                    max_ratio = matcher.ratio();
                    self.character_name_list[player_number as usize] = ( character_name.clone(), max_ratio );
                    if 1.0 <= max_ratio {
                        break;
                    }
                }
            }
        }

        println!("\r{} => {:?}", maybe_character_name, self.character_name_list[player_number as usize]);
    }
    /// プレイヤーが使用しているキャラクターは確定しているか
    pub fn is_decided_character_name(&self, player_number: i32) -> bool {
        1.0 <= self.character_name_list[player_number as usize].1
    }

    /// プレイヤーのストックの設定
    pub fn set_stock(&mut self, player_number: i32, stock: i32) {
        if self.stock_list[player_number as usize].0 == stock {
            return;
        }

        if -1 == self.stock_list[player_number as usize].0 {
            // 初回代入
            self.stock_list[player_number as usize] = ( stock, 1.0 );
            if -1 == self.max_stock_list[player_number as usize] {
                self.max_stock_list[player_number as usize] = stock;
            }
        }

        // 出鱈目な数値が来ると確定度合いを下げる (0未満 or 現在よりストックが増えた状態)
        if stock < 0 || self.stock_list[player_number as usize].0 < stock {
            self.stock_list[player_number as usize].1 /= 2.0;
        } else if stock == self.stock_list[player_number as usize].0 - 1 {
            // ストックはデクリメントしかされない保証
            self.stock_list[player_number as usize].0 = stock;
        }

        println!("{} : {} => {:?}", player_number, stock, self.stock_list[player_number as usize]);
    }
}


/* 戦歴を管理するクラス */
static DATABASE_NAME: &str = "smabrog_battle_history";
pub struct BattleHistory {
    client: Client,
    database: Database,
}
impl Default for BattleHistory {
    fn default() -> Self { Self::new() }
}
impl BattleHistory {
    fn new() -> Self {
        // MongoDBへの接続(の代わりに作成)とdatabaseの取得
        let options = ClientOptions::builder()
            .hosts(vec![
                StreamAddress {
                    hostname: "localhost".into(),
                    port: Some(27017),
                }
            ])
            .build();
        let client = Client::with_options(options).unwrap();
        let database = client.database(DATABASE_NAME);

        Self {
            client: client,
            database: database,
        }
    }
}

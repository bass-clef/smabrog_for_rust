
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
static CHARACTER_NAME_UNKNOWN: &str = "unknown";

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
    start_time: Option<std::time::Instant>,
    end_time: Option<std::time::Instant>,
    
    player_count: i32,
    rule_name: BattleRule,

    // プレイヤーの数だけ存在するデータ
    character_name_list: Vec<(String, f32)>,
    stock_list: Vec<(i32, f32)>,
    order_list: Vec<(i32, f32)>,
    power_list: Vec<(i32, f32)>,
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
            rule_name: BattleRule::Unknown,
            start_time: None,
            end_time: None,

            character_name_list: vec![("".to_string(), 0.0)],
            stock_list: vec![],
            order_list: vec![],
            power_list: vec![],
            group_list: vec![],

            max_stock_list: vec![],
        }
    }

    /// プレイヤーデータの初期化 (ReadyToFight 系で呼んで)
    pub fn initialize_battle(&mut self, player_count: i32) {
        if CHARACTER_NAME_UNKNOWN == self.character_name_list[0].0 {
            // 初期化してあったら処理しない(全部同時に初期化されるので character_name_list だけで比較)
            return;
        }

        println!("new battle with {}", player_count);
        self.start_time = Some(std::time::Instant::now());
        self.player_count = player_count;
        self.character_name_list.clear();
        self.stock_list.clear();
        self.order_list.clear();
        self.power_list.clear();
        self.group_list.clear();
        for _ in 0..self.player_count {
            self.character_name_list.push( (CHARACTER_NAME_UNKNOWN.to_string(), 0.0 ) );
            self.stock_list.push( (-1, 0.0) );
            self.order_list.push( (-1, 0.0) );
            self.power_list.push( (-1, 0.0) );
            self.group_list.push( (PlayerGroup::Unknown, 0.0) );

            self.max_stock_list.push( -1 );
        }
    }

    pub fn get_player_count(&self) -> i32 { self.player_count }

    /// 試合中かどうか
    pub fn is_playing_battle(&self) -> bool {
        if let Some(end_time) = self.end_time {
            if end_time <= std::time::Instant::now() {
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
        start_time <= std::time::Instant::now()
    }

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
            let mut max_ratio = self.character_name_list[player_number as usize].1;
            let mut matcher = SequenceMatcher::new("", "");
            for (character_name, character_jpn) in self.smashbros_config.character_list.iter() {
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
    /// プレイヤーが使用しているキャラクターは確定しているか
    pub fn is_decided_character_name(&self, player_number: i32) -> bool {
        // 名前の一致度が 100% ならそれ以上は変更し得ない
        1.0 <= self.character_name_list[player_number as usize].1
    }
    /// 全員分が使用しているキャラクターは確定しているか
    pub fn all_decided_character_name(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_character_name(player_number) )
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
                if self.max_stock_list.iter().all(|&stock| 0 < stock) {
                    // 全員のストック数が確定すると最大値を推測する
                    if let Some(&max_stock) = self.max_stock_list.iter().max() {
                        self.max_stock_list.iter_mut().map(|_| max_stock);
                        println!("rule(stock): {}", max_stock);
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
    /// プレイヤーのストックは確定しているか
    pub fn is_decided_stock(&self, player_number: i32) -> bool {
        // ストック数が 1 の時はそれ以上減ることは仕様上ないはずなので skip
        1 == self.stock_list[player_number as usize].0
    }
    /// 全員分のストックは確定しているか
    pub fn all_decided_stock(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_stock(player_number) )
    }

    /// プレイヤーの順位の設定
    pub fn set_order(&mut self, player_number: i32, maybe_order: i32) {
        if self.is_decided_order(player_number) {
            return;
        }
        
        // ストック数と矛盾がなければ確定
        let player_stock = &self.stock_list[player_number as usize];
        let under_order_player_count = self.player_count - self.stock_list.iter().filter(|&stock| player_stock > stock ).count() as i32;
        if -1 == self.order_list[player_number as usize].0 {
            if maybe_order == under_order_player_count {
                self.order_list[player_number as usize] = ( maybe_order, 1.0 );
            } else {
                self.order_list[player_number as usize] = ( maybe_order, 0.1 );
            }
        } else {
            if maybe_order == under_order_player_count {
                // 初回矛盾でも仏の名のもとに 3 回償えば許される()
                self.order_list[player_number as usize].1 += 0.31;
            } else {
                // 馬鹿は死んでも治らない(負数になっても止めない)
                self.order_list[player_number as usize].1 -= 0.31;
            }
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

        println!("order {}p: {}? => {:?}", player_number+1, maybe_order, self.order_list[player_number as usize]);
    }
    /// プレイヤーの順位は確定しているか
    pub fn is_decided_order(&self, player_number: i32) -> bool {
        1.0 <= self.order_list[player_number as usize].1
    }
    /// 全員分の順位は確定しているか
    pub fn all_decided_order(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_order(player_number) )
    }

    /// プレイヤーの戦闘力の設定 (一桁以下は無視)
    pub fn set_power(&mut self, player_number: i32, maybe_power: i32) {
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
    /// プレイヤーの戦闘力は確定しているか
    pub fn is_decided_power(&self, player_number: i32) -> bool {
        1.0 <= self.power_list[player_number as usize].1
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

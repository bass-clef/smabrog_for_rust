
use chrono::DateTime;
use difflib::sequencematcher::SequenceMatcher;
use mongodb::*;
use mongodb::options::{
    ClientOptions, Credential, StreamAddress,
};
use serde::{
    Deserialize, Serialize,
};
use std::collections::HashMap;
use std::io::{
    BufReader,
};

use crate::data::bson::*;
use crate::gui::GUI;

// スマブラ情報が入ったコンフィグデータ
static SMASHBROS_CHARACTER_LIST_FILE_NAME: &str = "smashbros_config.json";
#[derive(Serialize, Deserialize, Debug)]
pub struct SmashbrosConfig {
    version: String,
    character_list: HashMap<String, String>,
}
impl Default for SmashbrosConfig {
    fn default() -> Self {
        Self::new()
    }
}
impl SmashbrosConfig {
    fn new() -> Self {
        let file = std::fs::File::open(SMASHBROS_CHARACTER_LIST_FILE_NAME).unwrap();
        let reader = BufReader::new(file);
        
        match serde_json::from_reader::<_, Self>(reader) {
            Ok(config) => {
                println!("loaded config version [{}.*.*]", config.version);
                config
            },
            Err(_) => {
                panic!("invalid smashbros_config.");
            }
        }
    }
}
/// シングルトンでコンフィグを保持するため
pub struct WrappedSmashbrosConfig {
    smashbros_config: Option<SmashbrosConfig>
}
impl WrappedSmashbrosConfig {
    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    pub fn get(&mut self) -> &SmashbrosConfig {
        if self.smashbros_config.is_none() {
            self.smashbros_config = Some(SmashbrosConfig::new());
        }
        self.smashbros_config.as_ref().unwrap()
    }
}
pub static mut SMASHBROS_CONFIG: WrappedSmashbrosConfig = WrappedSmashbrosConfig {
    smashbros_config: None,
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
    pub fn insert_with_2(&mut self, data: &SmashbrosData) {
        let database = self.db_client.database("smabrog-db");
        let collection_ref = database.collection("battle_data_with_2_col").clone();
        async_std::task::block_on(async move {
            match collection_ref.insert_one(doc! {
                "start_time": format!( "{:?}", data.get_start_time().unwrap_or(chrono::Local::now()) ),
                "end_time": format!( "{:?}", data.get_end_time().unwrap_or(chrono::Local::now()) ),
                "rule": format!("{:?}", data.get_rule()),
                "max_stock": data.get_max_stock(0),
                "player": &data.get_character(0), "opponent": data.get_character(1),
                "stock": data.get_stock(0), "stock_diff": data.get_stock(0) - data.get_stock(1),
                "order": data.get_order(0),
                "power": data.get_power(0), "power_diff": data.get_power(0) - data.get_power(1),
            }, None).await {
                Ok(ret) => println!("[ok] finished battle. {:?}", ret),
                Err(e) => println!("[err] {:?}", e),
            }
        });
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
// ルール
#[derive(Debug, Clone)]
pub enum BattleRule {
    Unknown, Time, Stock, HealthPoint,
}
static CHARACTER_NAME_UNKNOWN: &str = "unknown";
/// 収集したデータ郡
#[derive(Debug)]
pub struct SmashbrosData {
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
impl SmashbrosData {
    fn new() -> Self {
        Self {
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
        if CHARACTER_NAME_UNKNOWN == self.character_name_list[0].0 {
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

        println!("new battle with {}", player_count);
        self.player_count = player_count;

        // 初期値代入
        for _ in 0..self.player_count {
            self.character_name_list.push( (CHARACTER_NAME_UNKNOWN.to_string(), 0.0 ) );
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

    /// 試合開始時刻の取得
    pub fn get_start_time(&self) -> Option<DateTime<chrono::Local>> { self.start_time.clone() }
    /// 試合終了時刻の取得
    pub fn get_end_time(&self) -> Option<DateTime<chrono::Local>> { self.end_time.clone() }
    /// 試合開始の設定 (ReadyToFight,Match 系で呼ぶ)
    pub fn start_battle(&mut self) {
        self.start_time = Some(chrono::Local::now());
    }
    /// 試合終了の設定 (GameEnd,Result 系で呼ぶ)
    pub fn finish_battle(&mut self) {
        self.end_time = Some(chrono::Local::now());
    }
    /// 試合情報の保存
    pub fn save_battle(&self) {
        match self.player_count {
            2 => unsafe{BATTLE_HISTORY.get_mut()}.insert_with_2(self),
            _ => (),
        };
    }
    /// 試合中かどうか
    pub fn is_playing_battle(&self) -> bool {
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
    /// 試合後かどうか
    pub fn is_finished_battle(&self) -> bool {
        if self.start_time.is_none() {
            return false;
        }

        let end_time = match self.end_time {
            Some(end_time) => end_time,
            None => return false,
        };

        end_time <= chrono::Local::now()
    }

    /// プレイヤー数の取得
    pub fn get_player_count(&self) -> i32 { self.player_count }

    /// ルールの取得
    pub fn get_rule(&self) -> BattleRule { self.rule_name.clone() }

    /// プレイヤーのキャラクターの取得
    pub fn get_character(&self, player_number: i32) -> String { self.character_name_list[player_number as usize].0.clone() }
    /// プレイヤーが使用しているキャラクターの設定
    pub fn set_character_name(&mut self, player_number: i32, maybe_character_name: String) {
        if 1.0 <= self.character_name_list[player_number as usize].1 {
            // 一致度が 100% だと比較しない
            return;
        }
        
        if unsafe{SMASHBROS_CONFIG.get()}.character_list.contains_key(&maybe_character_name) {
            // O(1)
            self.character_name_list[player_number as usize] = ( maybe_character_name.clone(), 1.0 );
        } else {
            // O(1+N)
            let mut max_ratio = self.character_name_list[player_number as usize].1;
            let mut matcher = SequenceMatcher::new("", "");
            for (character_name, _) in unsafe{SMASHBROS_CONFIG.get()}.character_list.iter() {
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
        !self.character_name_list.is_empty() && 1.0 <= self.character_name_list[player_number as usize].1
    }
    /// 全員分が使用しているキャラクターは確定しているか
    pub fn all_decided_character_name(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_character_name(player_number) )
    }

    /// プレイヤーのグループの取得
    pub fn get_group(&self, player_number: i32) -> PlayerGroup { self.group_list[player_number as usize].0.clone() }

    /// プレイヤーの最大ストック数の取得
    pub fn get_max_stock(&self, player_number: i32) -> i32 { self.max_stock_list[player_number as usize] }
    /// プレイヤーのストック数の取得
    pub fn get_stock(&self, player_number: i32) -> i32 { self.stock_list[player_number as usize].0 }
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
    /// プレイヤーのストックは確定しているか
    pub fn is_decided_stock(&self, player_number: i32) -> bool {
        // ストック数が 1 の時はそれ以上減ることは仕様上ないはずなので skip
        !self.stock_list.is_empty() && 1 == self.stock_list[player_number as usize].0
    }
    /// 全員分のストックは確定しているか
    pub fn all_decided_stock(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_stock(player_number) )
    }

    /// プレイヤーの順位の取得
    pub fn get_order(&self, player_number: i32) -> i32 { self.order_list[player_number as usize].0 }
    /// プレイヤーの順位の設定
    pub fn set_order(&mut self, player_number: i32, maybe_order: i32) {
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

        println!("order {}p: {}? => {:?}", player_number+1, maybe_order, self.order_list[player_number as usize]);
    }
    /// プレイヤーの順位は確定しているか
    pub fn is_decided_order(&self, player_number: i32) -> bool {
        !self.order_list.is_empty() && 1.0 <= self.order_list[player_number as usize].1
    }
    /// 全員分の順位は確定しているか
    pub fn all_decided_order(&self) -> bool {
        (0..self.player_count).collect::<Vec<i32>>().iter().all( |&player_number| self.is_decided_order(player_number) )
    }

    /// プレイヤーの順位の取得
    pub fn get_power(&self, player_number: i32) -> i32 { self.power_list[player_number as usize].0 }
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
        !self.power_list.is_empty() && 1.0 <= self.power_list[player_number as usize].1
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

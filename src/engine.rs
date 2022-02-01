
use linked_hash_map::LinkedHashMap;


use crate::capture::*;
use crate::data::*;
use crate::resource::battle_history;
use crate::scene::*;


/// スマブラを管理するコントローラークラス
pub struct SmashBrogEngine {
    scene_manager: SceneManager,
    prev_saved_time: std::time::Instant,
    prev_find_chara_list: Vec<String>,
    data_latest_10: Vec<SmashbrosData>,
    data_latest_by_chara: Vec<SmashbrosData>,
    data_all_by_chara: Vec<SmashbrosData>,
}
impl Default for SmashBrogEngine {
    fn default() -> Self { Self::new() }
}
impl SmashBrogEngine {
    const FIND_LIMIT: i64 = 1000;
    const GET_LIMIT: i64 = 10000;

    fn new() -> Self {
        let mut own = Self {
            scene_manager: SceneManager::default(),
            prev_saved_time: std::time::Instant::now(),
            prev_find_chara_list: Vec::new(),
            data_latest_10: battle_history().get().find_data_limit_10().unwrap_or(Vec::new()),
            data_latest_by_chara: Vec::new(),
            data_all_by_chara: Vec::new(),
        };

        // 初回に対キャラを表示させるために、前回のキャラ名を取得して、それを prev につっこんどく
        if let Some(data_latest_10) = battle_history().get().find_data_limit_10() {
            if 0 < data_latest_10.len() && 2 == data_latest_10[0].get_player_count() && data_latest_10[0].all_decided_character_name() {
                own.prev_find_chara_list = vec![data_latest_10[0].get_character(0), data_latest_10[0].get_character(1)];
            }
        }

        own
    }

    /// 現在のデータから更新があったら更新する
    pub fn update_now_data(&mut self) -> bool {
        let prev_find_chara_list = vec![
            self.get_now_data().get_character(0), self.get_now_data().get_character(1)
        ];

        if prev_find_chara_list == self.prev_find_chara_list {
            let prev_saved_time = match self.get_now_data().get_saved_time() {
                None => return false,
                Some(prev_saved_time) => prev_saved_time,
            };
            if self.prev_saved_time == prev_saved_time {
                return false;
            }
            self.prev_saved_time = prev_saved_time;
        }

        if let Some(data_latest_10) = battle_history().get().find_data_limit_10() {
            self.data_latest_10 = data_latest_10;
        }
        if !self.prev_find_chara_list.is_empty() && !self.prev_find_chara_list[0].is_empty() && self.prev_find_chara_list[0] != SmashbrosData::CHARACTER_NAME_UNKNOWN {
            if let Some(data_latest_by_chara) = battle_history().get().find_data_by_chara_list(self.prev_find_chara_list.clone(), Self::FIND_LIMIT, false) {
                self.data_latest_by_chara = data_latest_by_chara;
            }
            if let Some(data_all_by_chara) = battle_history().get().find_data_by_chara_list(self.prev_find_chara_list.clone(), Self::GET_LIMIT, true) {
                self.data_all_by_chara = data_all_by_chara;
            }
        }

        log::info!("now updated. {}, {}, {}", self.data_latest_10.len(), self.data_latest_by_chara.len(), self.data_all_by_chara.len());
        self.prev_find_chara_list = prev_find_chara_list;

        true
    }

    /// 検出方法の変更
    /// @result bool 0:問題なし
    pub fn change_capture_mode(&mut self, capture_mode: &CaptureMode) -> opencv::Result<()> {
        if capture_mode.is_default() {
            return Err(opencv::Error::new(opencv::core::StsError, "is default capture mode".to_string()));
        }

        let capture: opencv::Result<Box<dyn CaptureTrait>> = match capture_mode {
            CaptureMode::Desktop(_) => match CaptureFromDesktop::new() {
                Err(e) => return Err(e),
                Ok(capture) => Ok(Box::new(capture)),
            },
            CaptureMode::Empty(_) => Ok(Box::new(CaptureFromEmpty::new().unwrap())),
            CaptureMode::VideoDevice(_, device_id, _) => match CaptureFromVideoDevice::new(*device_id) {
                Err(e) => return Err(e),
                Ok(capture) => Ok(Box::new(capture)),
            },
            CaptureMode::Window(_, win_caption) => match CaptureFromWindow::new(win_caption) {
                Err(e) => return Err(e),
                Ok(capture) => Ok(Box::new(capture)),
            },
        };

        if let Ok(capture) = capture {
            self.scene_manager.capture = capture;
        }

        Ok(())
    }

    /// 言語の変更
    pub fn change_language(&mut self) {
        self.scene_manager.change_language();
    }

    /// 直近 10 件のデータを返す (10未満も返る)
    /// @result Vec<SmashbrosData> 取得していたデータ郡の clone
    pub fn get_data_latest_10(&mut self) -> Vec<SmashbrosData> {
        self.update_now_data();

        self.data_latest_10.clone()
    }

    /// 現在キャラクター指定での直近 FIND_LIMIT 件のデータを返す
    pub fn get_data_latest_by_now_chara(&mut self) -> Vec<SmashbrosData> {
        self.update_now_data();

        self.data_latest_by_chara.clone()
    }

    /// 現在キャラクター指定での全データを返す (一応上限を GET_LIMIT で定めておく)
    pub fn get_data_all_by_now_chara(&mut self) -> Vec<SmashbrosData> {
        self.update_now_data();

        self.data_all_by_chara.clone()
    }

    /// 指定データの (勝率, 試合数) を返す
    pub fn get_wins_by_data_list(&mut self, data_list: Vec<SmashbrosData>) -> (f32, i32) {
        let mut battle_count = 0.0;
        let battle_rate = data_list.iter().map(|data| {
            if data.get_player_count() != 2 || !data.all_decided_order() || -1 == data.get_order(0) || -1 == data.get_order(1) || data.get_order(0) == data.get_order(1) {
                // draw or unknown
                return 0.0;
            }

            battle_count += 1.0;
            if data.get_order(0) < data.get_order(1) {
                // win
                1.0
            } else {
                // lose
                0.0
            }
        }).sum::<f32>();

        if battle_count == 0.0 {
            return (0.0, 0);
        }

        (battle_rate / battle_count, battle_count as i32)
    }

    /// 指定データの (勝率, 試合数) をキャラ別に分けて返す
    pub fn get_wins_by_data_list_groupby_character(&mut self, data_list: Vec<SmashbrosData>) -> LinkedHashMap<String, (f32, i32)> {
        let mut data_list_by_chara = LinkedHashMap::new();
        for data in data_list {
            let chara_name = data.get_character(1).clone();
            data_list_by_chara.entry(chara_name).or_insert(Vec::new()).push(data);
        }

        let mut result = LinkedHashMap::new();
        for data in data_list_by_chara {
            *result.entry(data.0).or_insert((0.0, 0)) = self.get_wins_by_data_list(data.1);
        }

        result
    }

    /// 直近 10 件の(勝数, 負数)を返す
    pub fn get_win_lose_latest_10(&mut self) -> (i32, i32) {
        self.get_data_latest_10().iter().fold((0, 0), |(mut win, mut lose), data| {
            if data.get_player_count() == 2 || data.get_order(0) != -1 {
                if data.get_order(0) < data.get_order(1) {
                    // win
                    win += 1;
                } else {
                    // lose
                    lose += 1;
                }
            }
            (win, lose)
        })
    }

    /// 現在対戦中のデータを返す
    pub fn get_now_data(&self) -> SmashbrosData {
        self.scene_manager.get_now_data()
    }

    /// 現在検出中のシーン名を返す
    pub fn get_captured_scene(&self) -> SceneList {
        self.scene_manager.get_now_scene()
    }

    /// 検出しようとしたシーンの前回の一致度合いを返す
    pub fn get_prev_match_ratio(&self) -> f64 {
        self.scene_manager.get_prev_match_ratio()
    }

    /// どっかのメインループで update する用
    pub fn update(&mut self) -> opencv::Result<()> {
        Ok( self.scene_manager.update_scene_list()? )
    }
}

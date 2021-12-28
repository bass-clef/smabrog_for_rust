
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
}
impl Default for SmashBrogEngine {
    fn default() -> Self { Self::new() }
}
impl SmashBrogEngine {
    fn new() -> Self {
        Self {
            scene_manager: SceneManager::default(),
            prev_saved_time: std::time::Instant::now(),
            prev_find_chara_list: Vec::new(),
            data_latest_10: battle_history().get().find_data_limit_10().unwrap_or(Vec::new()),
            data_latest_by_chara: Vec::new(),
        }
    }

    /// 現在のデータから更新があったら更新する
    pub fn update_now_data(&mut self) -> bool {
        let prev_saved_time = match self.get_now_data().get_saved_time() {
            None => return false,
            Some(prev_saved_time) => prev_saved_time,
        };
        if self.prev_saved_time == prev_saved_time {
            return false;
        }

        if let Some(data_latest_10) = battle_history().get().find_data_limit_10() {
            self.data_latest_10 = data_latest_10;
        }
        if let Some(data_latest_by_chara) = battle_history().get().find_data_by_chara_list(self.prev_find_chara_list.clone()) {
            self.data_latest_by_chara = data_latest_by_chara;
        }

        log::info!("now updated.");
        self.prev_saved_time = prev_saved_time;

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

    /// 現在キャラクター指定での直近 100 件のデータを返す (100未満も返る)
    pub fn get_data_latest_500_by_now_chara(&mut self) -> Vec<SmashbrosData> {
        let prev_find_chara_list = vec![
            self.get_now_data().get_character(0), self.get_now_data().get_character(1)
        ];

        self.update_now_data();
        if prev_find_chara_list != self.prev_find_chara_list {
            self.prev_find_chara_list = prev_find_chara_list;
            if let Some(data_latest_by_chara) = battle_history().get().find_data_by_chara_list(self.prev_find_chara_list.clone()) {
                self.data_latest_by_chara = data_latest_by_chara;
            }
        }

        self.data_latest_by_chara.clone()
    }

    /// 指定データの勝率を返す
    pub fn get_wins_by_data_list(&mut self, data_list: Vec<SmashbrosData>) -> f32 {
        let mut battle_count = 0.0;
        data_list.iter().map(|data| {
            if data.get_player_count() != 2 || data.get_order(0) == -1 {
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
        }).sum::<f32>() / battle_count
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

    pub fn get_prev_match_ratio(&self) -> f64 {
        self.scene_manager.get_prev_match_ratio()
    }

    /// どっかのメインループで update する用
    pub fn update(&mut self) -> opencv::Result<()> {
        Ok( self.scene_manager.update_scene_list()? )
    }
}

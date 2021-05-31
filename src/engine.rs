
use crate::capture::*;
use crate::data::*;
use crate::gui::*;
use crate::scene::*;


/// スマブラを管理するコントローラークラス
pub struct SmashBrogEngine {
    scene_manager: SceneManager,
    prev_saved_time: std::time::Instant,
    data_latest_10: Vec<SmashbrosData>,
}
impl Default for SmashBrogEngine {
    fn default() -> Self { Self::new() }
}
impl SmashBrogEngine {
    fn new() -> Self {
        Self {
            scene_manager: SceneManager::default(),
            prev_saved_time: std::time::Instant::now(),
            data_latest_10: unsafe{BATTLE_HISTORY.get()}.find_with_2_limit_10().unwrap(),
        }
    }

    /// 検出方法の変更
    /// @result bool 0:問題なし
    pub fn change_capture_mode(&mut self, capture_mode: &CaptureMode) -> opencv::Result<()> {
        let capture: opencv::Result<Box<dyn CaptureTrait>> = match capture_mode {
            CaptureMode::Desktop(_) => match CaptureFromDesktop::new() {
                Err(e) => return Err(opencv::Error::new(0, format!("{:?}", e))),
                Ok(capture) => Ok(Box::new(capture)),
            },
            CaptureMode::Empty(_) => Ok(Box::new(CaptureFromEmpty::new().unwrap())),
            CaptureMode::VideoDevice(_, device_id, _) => match CaptureFromVideoDevice::new(*device_id) {
                Err(e) => return Err(opencv::Error::new(0, format!("{:?}", e))),
                Ok(capture) => Ok(Box::new(capture)),
            },
            CaptureMode::Window(_, win_caption, win_class) => match CaptureFromWindow::new(win_caption, win_class) {
                Err(e) => return Err(opencv::Error::new(0, format!("{:?}", e))),
                Ok(capture) => Ok(Box::new(capture)),
            },
        };

        if let Ok(capture) = capture {
            self.scene_manager.capture = capture;
        }

        Ok(())
    }

    /// 直近 10 件のデータを返す
    /// @result Vec<SmashbrosData> 取得していたデータ郡の clone
    pub fn get_data_latest_10(&mut self) -> Vec<SmashbrosData> {
        if self.is_updated_now_data() {
            if let Some(data_latest_10) = unsafe{BATTLE_HISTORY.get()}.find_with_2_limit_10() {
                self.data_latest_10 = data_latest_10;
            }
        }

        self.data_latest_10.clone()
    }

    /// 現在対戦中のデータを返す
    pub fn get_now_data(&self) -> SmashbrosData {
        self.scene_manager.get_now_data()
    }

    /// 現在のデータから更新があったかどうか
    pub fn is_updated_now_data(&mut self) -> bool {
        let prev_saved_time = match self.get_now_data().get_saved_time() {
            None => return false,
            Some(prev_saved_time) => prev_saved_time,
        };

        if self.prev_saved_time == prev_saved_time {
            return false;
        }
        println!("now updated.");

        self.prev_saved_time = prev_saved_time;

        true
    }

    /// どっかのメインループで update する用
    pub fn update(&mut self) -> opencv::Result<Option<Message>> {
        Ok( self.scene_manager.update()? )
    }
}

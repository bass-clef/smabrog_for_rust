
use linked_hash_map::LinkedHashMap;


use crate::capture::*;
use crate::data::*;
use crate::resource::{
    BATTLE_HISTORY,
    GUI_CONFIG,
};
use crate::scene::*;


/// スマブラを管理するコントローラークラス
pub struct SmashBrogEngine {
    data_latest: Vec<SmashbrosData>,
    data_latest_by_chara: Vec<SmashbrosData>,
    data_all_by_chara: Vec<SmashbrosData>,
    result_max: i64,
    is_updated: bool,
}
impl Default for SmashBrogEngine {
    fn default() -> Self { Self::new() }
}
impl SmashBrogEngine {
    pub const DEFAULT_RESULT_LIMIT: i64 = 10;
    const GET_LIMIT: i64 = 1000;
    const FIND_LIMIT: i64 = 10000;

    pub const fn get_default_result_limit() -> i64 { Self::DEFAULT_RESULT_LIMIT }

    fn new() -> Self {
        let mut own = Self {
            data_latest: BATTLE_HISTORY().get_mut().find_data_limit(Self::DEFAULT_RESULT_LIMIT).unwrap_or(Vec::new()),
            data_latest_by_chara: Vec::new(),
            data_all_by_chara: Vec::new(),
            result_max: Self::DEFAULT_RESULT_LIMIT,
            is_updated: false,
        };

        // 更新するタイミングを登録
        SCENE_MANAGER().get_mut().registory_scene_event(
            SceneList::Unknown, SceneList::EndResultReplay,
            Box::new(|_smashbros_data: &mut SmashbrosData| SMASHBROS_ENGINE().get_mut().update_now_data() ),
        );
        own.update_now_data();

        own
    }

    /// 指定データの (勝数, 負数) を返す
    pub fn get_win_lose_by_data_list(data_list: &Vec<SmashbrosData>) -> (i32, i32) {
        data_list.iter().fold((0, 0), |(mut win, mut lose), data| {
            if let Some(is_win) = data.is_win() {
                if is_win {
                    win += 1;
                } else {
                    lose += 1;
                }
            }

            (win, lose)
        })
    }

    /// 指定データの (勝率, 試合数) を返す
    pub fn get_wins_by_data_list(data_list: &Vec<SmashbrosData>) -> (f32, i32) {
        let mut battle_count = 0.0;
        let battle_rate = data_list.iter().map(|data| {
            if let Some(is_win) = data.is_win() {
                battle_count += 1.0;
                if is_win {
                    1.0
                } else {
                    0.0
                }
            } else {
                0.0
            }
        }).sum::<f32>();

        if battle_count == 0.0 {
            return (0.0, 0);
        }

        (battle_rate / battle_count, battle_count as i32)
    }

    /// 指定データの (勝率, 試合数) をキャラ別に分けて返す
    pub fn get_wins_by_data_list_groupby_character(data_list: &Vec<SmashbrosData>) -> LinkedHashMap<String, (f32, i32)> {
        let mut data_list_by_chara = LinkedHashMap::new();
        for data in data_list {
            let chara_name = data.get_character(1).clone();
            data_list_by_chara.entry(chara_name).or_insert(Vec::new()).push(data.clone());
        }

        let mut result = LinkedHashMap::new();
        for data in data_list_by_chara {
            *result.entry(data.0).or_insert((0.0, 0)) = Self::get_wins_by_data_list(&data.1);
        }

        result
    }

    /// 現在のデータから更新があったかどうか
    pub fn is_update_now_data(&mut self) -> bool {
        self.is_updated
    }

    /// 現在のデータから更新があったら更新する
    pub fn update_now_data(&mut self) {
        self.update_latest_n_data();
        self.update_chara_find_data();

        self.is_updated = true;
        log::info!("now updated. {}, {}, {}", self.data_latest.len(), self.data_latest_by_chara.len(), self.data_all_by_chara.len());
    }

    /// 直近 result_max 件のデータを更新する
    pub fn update_latest_n_data(&mut self){
        if let Some(data_latest) = BATTLE_HISTORY().get_mut().find_data_limit(self.result_max) {
            self.data_latest = data_latest;
        }
        self.is_updated = true;
    }

    /// キャラ検索のデータを更新する
    pub fn update_chara_find_data(&mut self) {
        let prev_chara_list = vec![
            self.get_now_data().get_character(0), self.get_now_data().get_character(1)
        ];
        if let Some(data_latest_by_chara) = BATTLE_HISTORY().get_mut().find_data_by_chara_list(prev_chara_list.clone(), Self::GET_LIMIT, false) {
            self.data_latest_by_chara = data_latest_by_chara;
        }
        if let Some(data_all_by_chara) = BATTLE_HISTORY().get_mut().find_data_by_chara_list(prev_chara_list.clone(), Self::FIND_LIMIT, true) {
            self.data_all_by_chara = data_all_by_chara;
        }
        self.is_updated = true;
    }

    /// どっかのメインループで update する用
    pub fn update(&mut self) -> anyhow::Result<()> {
        self.is_updated = false;

        Ok( SCENE_MANAGER().get_mut().update_scene_list()? )
    }

    /// 検出方法の変更
    /// @result bool 0:問題なし
    pub fn change_capture_mode(&mut self, capture_mode: &CaptureMode) -> anyhow::Result<()> {
        if capture_mode.is_default() {
            anyhow::bail!("is default capture mode");
        }

        let capture: opencv::Result<Box<dyn CaptureTrait>> = match capture_mode {
            CaptureMode::Desktop(_) => match CaptureFromDesktop::new() {
                Err(e) => anyhow::bail!(e),
                Ok(capture) => Ok(Box::new(capture)),
            },
            CaptureMode::Empty(_) => Ok(Box::new(CaptureFromEmpty::new().unwrap())),
            CaptureMode::VideoDevice(_, device_id, _) => match CaptureFromVideoDevice::new(*device_id) {
                Err(e) => anyhow::bail!(e),
                Ok(capture) => Ok(Box::new(capture)),
            },
            CaptureMode::Window(_, win_caption) => match CaptureFromWindow::new(win_caption) {
                Err(e) => anyhow::bail!(e),
                Ok(capture) => Ok(Box::new(capture)),
            },
        };

        if let Ok(capture) = capture {
            SCENE_MANAGER().get_mut().capture = capture;
        }

        Ok(())
    }

    /// 言語の変更
    pub fn change_language(&mut self) {
        SCENE_MANAGER().get_mut().change_language();
    }

    /// 限界取得数の変更
    pub fn change_result_max(&mut self) {
        if self.result_max == GUI_CONFIG().get_mut().result_max {
            return;
        }
        self.update_latest_n_data();
        self.result_max = GUI_CONFIG().get_mut().result_max;
    }

    /// シーンイベントの登録
    pub fn registory_scene_event(&mut self, before_scene: SceneList, after_scene: SceneList, scene_event: SceneEventCallback) {
        SCENE_MANAGER().get_mut().registory_scene_event(before_scene, after_scene, scene_event);
    }

    // 現在の検出されたデータの参照を返す
    pub fn ref_now_data(&self) -> &SmashbrosData {
        SCENE_MANAGER().get_mut().ref_now_data()
    }

    /// 直近 result_max 件のデータを返す (result_max 未満も返る)
    /// @result Vec<SmashbrosData> 取得していたデータ郡の clone
    pub fn get_data_latest(&mut self) -> Vec<SmashbrosData> {
        self.change_result_max();

        self.data_latest.clone()
    }

    /// 現在キャラクター指定での直近 GET_LIMIT 件のデータを返す
    pub fn get_data_latest_by_now_chara(&mut self) -> Vec<SmashbrosData> {
        self.data_latest_by_chara.clone()
    }

    /// 現在キャラクター指定での全データを返す (一応上限を FIND_LIMIT で定めておく)
    pub fn get_data_all_by_now_chara(&mut self) -> Vec<SmashbrosData> {
        self.data_all_by_chara.clone()
    }

    /// 現在対戦中のデータを返す
    pub fn get_now_data(&self) -> SmashbrosData {
        if self.data_latest.len() < 1 {
            SCENE_MANAGER().get_mut().get_now_data()
        } else {
            self.data_latest[0].clone()
        }
    }

    /// 現在検出中の Mat を返す
    pub fn get_now_image(&self) -> &opencv::core::Mat {
        SCENE_MANAGER().get_mut().get_now_image()
    }

    /// 現在検出中のシーン名を返す
    pub fn get_captured_scene(&self) -> SceneList {
        SCENE_MANAGER().get_mut().get_now_scene()
    }

    /// 検出しようとしたシーンの前回の一致度合いを返す
    pub fn get_prev_match_ratio(&mut self) -> f64 {
        SCENE_MANAGER().get_mut().get_prev_match_ratio()
    }
}

/// シングルトンで Engine を保持するため
pub struct WrappedSmashBrogEngine {
    smashbrog_engine: Option<SmashBrogEngine>,
}
impl WrappedSmashBrogEngine {
    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    pub fn get(&mut self) -> &SmashBrogEngine {
        if self.smashbrog_engine.is_none() {
            self.smashbrog_engine = Some(SmashBrogEngine::default());
        }
        self.smashbrog_engine.as_ref().unwrap()
    }

    // mut 版
    pub fn get_mut(&mut self) -> &mut SmashBrogEngine {
        if self.smashbrog_engine.is_none() {
            self.smashbrog_engine = Some(SmashBrogEngine::default());
        }
        self.smashbrog_engine.as_mut().unwrap()
    }
}
static mut _SMASHBROS_ENGINE: WrappedSmashBrogEngine = WrappedSmashBrogEngine {
    smashbrog_engine: None,
};
#[allow(non_snake_case)]
pub fn SMASHBROS_ENGINE() -> &'static mut WrappedSmashBrogEngine {
    unsafe { &mut _SMASHBROS_ENGINE }
}

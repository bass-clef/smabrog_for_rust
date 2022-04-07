
use opencv::{
    core,
    imgcodecs,
    imgproc,
    prelude::*
};
use std::collections::HashMap;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::capture::*;
use crate::data::*;
use crate::utils::utils;

pub mod judgment;
pub mod unknown;
pub mod loading;
pub mod dialog;
pub mod ready_to_fight;
pub mod matching;
pub mod ham_vs_spam;
pub mod game_start;
pub mod game_playing;
pub mod game_end;
pub mod result;

pub use judgment::SceneJudgment;
pub use unknown::UnknownScene;
pub use loading::LoadingScene;
pub use dialog::DialogScene;
pub use ready_to_fight::ReadyToFightScene;
pub use matching::MatchingScene;
pub use ham_vs_spam::HamVsSpamScene;
pub use game_start::GameStartScene;
pub use game_playing::GamePlayingScene;
pub use game_end::GameEndScene;
pub use result::ResultScene;


#[derive(Copy, Clone)]
pub enum ColorFormat {
    NONE = 0, GRAY = 1,
    RGB = 3, RGBA = 4,
}


/// シーンリスト
#[derive(Clone, Copy, Debug, EnumIter, Eq, Hash, PartialEq)]
pub enum SceneList {
    ReadyToFight = 0, Matching, HamVsSpam,
    GameStart, GamePlaying, GameEnd, Result,
    Dialog, Loading, Unknown,
    
    DecidedRules, DecidedBgm, EndResultReplay,
}
impl SceneList {
    /// i32 to SceneList
    fn to_scene_list(scene_id: i32) -> Self {
        for scene in SceneList::iter() {
            if scene as i32 == scene_id {
                return scene;
            }
        }

        SceneList::Unknown
    }
}
impl Default for SceneList {
    fn default() -> Self {
        Self::Unknown
    }
}


/// シーン雛形 (動作は子による)
pub trait SceneTrait: downcast::Any {
    /// 言語の変更
    fn change_language(&mut self) {}
    /// シーン識別ID
    fn get_id(&self) -> i32;
    /// 前回の検出情報
    fn get_prev_match(&self) -> Option<&SceneJudgment>;
    /// シーンを検出するか
    fn continue_match(&self, now_scene: SceneList) -> bool;
    /// "この"シーンかどうか
    fn is_scene(&mut self, mat: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool>;
    /// 次はどのシーンに移るか
    fn to_scene(&self, now_scene: SceneList) -> SceneList;
    /// シーンを録画する
    fn recoding_scene(&mut self, capture: &core::Mat) -> opencv::Result<()>;
    /// 録画されたかどうか
    fn is_recoded(&self) -> bool;
    /// シーン毎に録画したものから必要なデータを検出する
    fn detect_data(&mut self, smashbros_data: &mut SmashbrosData) -> opencv::Result<()>;
}
downcast::downcast!(dyn SceneTrait);


/// シーンイベントのコールバックの型
pub type SceneEventCallback = Box<dyn FnMut(&mut SmashbrosData) -> ()>;
pub type ManageEventCallback = Box< dyn FnMut(&mut SceneManager) -> Option<&mut Vec<SceneEventCallback>> >;
pub type ManageEventConditions = Box<dyn FnMut(&SceneManager) -> bool>;

struct ManageEventContent {
    pub init_conditions: ManageEventConditions,
    pub fire_conditions: ManageEventConditions,
    pub manage_event_callback: ManageEventCallback,
    pub is_fired: bool,
}

/// 現在の検出されたデータの変更可能参照を返す
/// 関数として書いたら借用違反で怒られるので
/// (Rust に C++ のような inline 関数ってないの???)
macro_rules! mut_now_data {
    ($scene_manager:ident, $scene_id:ident) => {
        match SceneList::to_scene_list($scene_id as i32) {
            SceneList::Matching => &mut $scene_manager.smashbros_data,
            _ => if $scene_manager.sub_smashbros_data != SmashbrosData::default() {
                &mut $scene_manager.sub_smashbros_data
            } else {
                &mut $scene_manager.smashbros_data
            },
        }
    };
}

/// シーン全体を非同期で管理するクラス
pub struct SceneManager {
    pub capture: Box<dyn CaptureTrait>,
    pub scene_loading: LoadingScene,
    pub scene_list: Vec<Box<dyn SceneTrait>>,
    pub now_scene: SceneList,
    pub smashbros_data: SmashbrosData,
    pub sub_smashbros_data: SmashbrosData,
    pub dummy_local_time: chrono::DateTime<chrono::Local>,
    pub prev_match_ratio: f64,
    pub prev_match_scene: SceneList,
    capture_image: core::Mat,
    scene_event_list: HashMap< (SceneList, SceneList), Vec<SceneEventCallback> >,
    manage_event_list: Vec<ManageEventContent>,
}
impl Default for SceneManager {
    fn default() -> Self {
        let mut own = Self {
            capture: Box::new(CaptureFromEmpty::new().unwrap()),
            scene_loading: LoadingScene::default(),
            scene_list: vec![
                Box::new(ReadyToFightScene::default()),
                Box::new(MatchingScene::default()),
                Box::new(HamVsSpamScene::default()),
                Box::new(GameStartScene::default()),
                Box::new(GamePlayingScene::default()),
                Box::new(GameEndScene::default()),
                Box::new(ResultScene::default()),
                Box::new(DialogScene::default()),
            ],
            now_scene: SceneList::default(),
            smashbros_data: SmashbrosData::default(),
            sub_smashbros_data: SmashbrosData::default(),
            dummy_local_time: chrono::Local::now(),
            prev_match_ratio: 0.0,
            prev_match_scene: SceneList::default(),
            capture_image: core::Mat::default(),
            scene_event_list: HashMap::new(),
            manage_event_list: Vec::new(),
        };

        // DecidedRules イベントを定義
        own.registory_manage_event(Box::new(|scene_manager: &SceneManager| {
            scene_manager.now_scene == SceneList::Matching
        }), Box::new(|scene_manager: &SceneManager| {
            // ルールと最大ストック、時間が決まった時に DecidedRules を発行
            scene_manager.ref_now_data().is_decided_rule_all_clause()
        }), Box::new(|scene_manager: &mut SceneManager| {
            log::info!("[{:?}] match [Unknown] to [DecidedRules]", scene_manager.now_scene);

            scene_manager.scene_event_list.get_mut(&(SceneList::Unknown, SceneList::DecidedRules))
        }));

        // DecidedBgm イベントを定義
        own.registory_manage_event(Box::new(|scene_manager: &SceneManager| {
            scene_manager.now_scene == SceneList::Matching
        }), Box::new(|scene_manager: &SceneManager| {
            // BGM 名が決まった時に DecidedBgm を発行
            scene_manager.ref_now_data().is_decided_bgm_name()
        }), Box::new(|scene_manager: &mut SceneManager| {
            log::info!("[{:?}] match [Unknown] to [DecidedBgm]", scene_manager.now_scene);

            scene_manager.scene_event_list.get_mut(&(SceneList::Unknown, SceneList::DecidedBgm))
        }));

        // EndResultReplay イベントを定義
        own.registory_manage_event(Box::new(|scene_manager: &SceneManager| {
            if let Ok(result_scene) = scene_manager.scene_list[SceneList::Result as usize].downcast_ref::<ResultScene>() {
                if result_scene.is_recoded() {
                    // Result シーンの録画が終わると初期化
                    return true;
                }
            }

            false
        }), Box::new(|scene_manager: &SceneManager| {
            let result_scene = match scene_manager.scene_list[SceneList::Result as usize].downcast_ref::<ResultScene>() {
                Ok(result_scene) => result_scene,
                Err(_) => return false,
            };

            // 結果画面のリプレイが終わった時に EndResultReplay を発行
            result_scene.buffer.is_replay_end() && scene_manager.sub_smashbros_data != SmashbrosData::default()
        }), Box::new(|scene_manager: &mut SceneManager| {
            log::info!("[{:?}] match [Unknown] to [EndResultReplay]", scene_manager.now_scene);

            scene_manager.scene_event_list.get_mut(&(SceneList::Unknown, SceneList::EndResultReplay))
        }));

        // 試合の開始と終了
        own.registory_scene_event(SceneList::HamVsSpam, SceneList::GamePlaying, Box::new(|smashbros_data: &mut SmashbrosData| {
            smashbros_data.start_battle();
        }));
        own.registory_scene_event(SceneList::GamePlaying, SceneList::GameEnd, Box::new(|smashbros_data: &mut SmashbrosData| {
            smashbros_data.finish_battle();
        }));

        // 初期ストックの代入
        own.registory_scene_event(SceneList::Unknown, SceneList::DecidedRules, Box::new(|smashbros_data: &mut SmashbrosData| {
            match smashbros_data.get_rule() {
                BattleRule::Stock | BattleRule::Stamina => {
                    for player_number in 0..smashbros_data.get_player_count() {
                        smashbros_data.set_stock(player_number, smashbros_data.get_max_stock(player_number));
                    }
                }
                _ => (),
            }
        }));

        // Result のリプレイが終わった時に一応 save/update しておく
        own.registory_scene_event(SceneList::Unknown, SceneList::EndResultReplay, Box::new(|_smashbros_data: &mut SmashbrosData| {
            if SCENE_MANAGER().get_mut().sub_smashbros_data.get_id().is_some() {
                SCENE_MANAGER().get_mut().sub_smashbros_data.update_battle();
            } else {
                SCENE_MANAGER().get_mut().sub_smashbros_data.save_battle();
            }

            // Tournament初期化されていなかったら sub を再び main にする
            if SCENE_MANAGER().get_mut().smashbros_data.is_finished_battle() {
                SCENE_MANAGER().get_mut().smashbros_data = SCENE_MANAGER().get_mut().sub_smashbros_data.clone();
            }
            SCENE_MANAGER().get_mut().sub_smashbros_data = SmashbrosData::default();
        }));

        own
    }
}
impl SceneManager {
    // 現在の検出されたデータの参照を返す
    pub fn ref_now_data(&self) -> &SmashbrosData {
        match SceneList::to_scene_list(self.now_scene as i32) {
            SceneList::Matching => &self.smashbros_data,
            _ => if self.sub_smashbros_data != SmashbrosData::default() {
                &self.sub_smashbros_data
            } else {
                &self.smashbros_data
            },
        }
    }

    // 現在の検出されたデータを返す
    pub fn get_now_data(&self) -> SmashbrosData {
        let mut cloned_data = self.ref_now_data().clone();

        // 重複操作されないために適当な時間で保存済みのデータにする
        cloned_data.set_id(Some("dummy_id".to_string()));
        
        cloned_data
    }

    // 現在の Mat を返す
    pub fn get_now_image(&self) -> &core::Mat {
        &self.capture_image
    }

    // 現在のシーンを返す
    pub fn get_now_scene(&self) -> SceneList {
        self.now_scene.clone()
    }

    // 次に検出予定のシーン
    pub fn get_next_scene(&self) -> SceneList {
        self.prev_match_scene.clone()
    }

    // 現在検出しようとしているシーンの、前回の検出率を返す
    pub fn get_prev_match_ratio(&mut self) -> f64 {
        self.prev_match_ratio = 0.0;
        for index in 0..self.scene_list.len() {
            if self.scene_list[index].continue_match(self.now_scene) {
                if let Some(scene_judgment) = self.scene_list[index].get_prev_match() {
                    if self.prev_match_ratio < scene_judgment.prev_match_ratio {
                        self.prev_match_ratio = scene_judgment.prev_match_ratio;
                        self.prev_match_scene = SceneList::to_scene_list(self.scene_list[index].get_id());
                    }
                }
            }
        }

        self.prev_match_ratio
    }

    // シーンが切り替わった際に呼ばれるイベントを登録する
    pub fn registory_scene_event(&mut self, before_scene: SceneList, after_scene: SceneList, callback: SceneEventCallback) {
        self.scene_event_list.entry((before_scene, after_scene)).or_insert(Vec::new()).push(callback);
    }

    // 複雑なイベントを登録する
    pub fn registory_manage_event(&mut self, init_conditions: ManageEventConditions, fire_conditions: ManageEventConditions, manage_event_callback: ManageEventCallback) {
        self.manage_event_list.push(ManageEventContent {
            init_conditions,
            fire_conditions,
            manage_event_callback,
            is_fired: true, // 初期状態では発火済みとして、初期化条件で初期化されるのを待つ
        });
    }

    // イベントの更新
    pub fn update_event(&mut self) {
        // 複雑なイベントの処理
        for manage_event in &mut self.manage_event_list {
            if manage_event.is_fired {
                // 発火済みなら初期化条件を満たすまで監視
                if manage_event.init_conditions.as_mut()(SCENE_MANAGER().get_mut()) {
                    manage_event.is_fired = false;
                }
            } else if manage_event.fire_conditions.as_mut()(SCENE_MANAGER().get_mut()) {
                // 発火条件を満たしたら manage_event_callback が返す SceneEventCallback のリストを実行
                manage_event.is_fired = true;

                let now_scene = self.now_scene.clone();
                if let Some(scene_event_list) = manage_event.manage_event_callback.as_mut()(SCENE_MANAGER().get_mut()) {
                    for scene_event in scene_event_list {
                        scene_event(mut_now_data!(self, now_scene));
                    }
                }
            }
        }
    }

    // シーンを更新する
    pub async fn update_scene<'a>(&mut self, capture_image: &'a core::Mat, index: usize, is_loading: bool) {
        // シーンによって適切な時に録画される
        self.scene_list[index].recoding_scene(&capture_image).unwrap_or(());
        if self.scene_list[index].is_recoded() {
            // 所謂ビデオ判定
            self.scene_list[index].detect_data(mut_now_data!(self, index)).unwrap_or(());
        }

        // 読込中の画面(真っ黒に近い)はテンプレートマッチングで 1.0 がでてしまうので回避
        // よけいな match をさけるため(is_scene すること自体が結構コストが高い)
        if is_loading || !self.scene_list[index].continue_match(self.now_scene) {
            return;
        }

        // 遷移?
        if self.scene_list[index].is_scene(&capture_image, Some(mut_now_data!(self, index))).unwrap_or(false) {
            let to_scene = self.scene_list[index].to_scene(self.now_scene);
            log::info!(
                "[{:?}]({:2.3}%) match {:?} to {:?}",
                SceneList::to_scene_list(self.scene_list[index].get_id()), self.scene_list[index].get_prev_match().unwrap().prev_match_ratio,
                self.now_scene, to_scene
            );

            // シーンが切り替わった際に呼ばれるイベントを発火
            if let Some(scene_event_list) = self.scene_event_list.get_mut(&(self.now_scene, to_scene)) {
                for event in scene_event_list {
                    event.as_mut()(mut_now_data!(self, index));
                }
            }

            if to_scene == SceneList::GameEnd {
                self.sub_smashbros_data = self.smashbros_data.clone();
            }

            self.now_scene = to_scene;
        }
    }

    // 全てのシーンを更新する
    pub fn update_scene_list(&mut self) -> anyhow::Result<()> {
        let capture_image = self.capture.get_mat()?;
        self.capture_image = capture_image.clone();

        let is_loading = self.scene_loading.is_scene(&capture_image, None)?;

        // 現在キャプチャと比較して遷移する
        for index in 0..self.scene_list.len() {
            async_std::task::block_on(async {
                self.update_scene(&capture_image, index, is_loading).await;
            });
        }

        self.update_event();

        Ok(())
    }

    // 言語の変更をする
    pub fn change_language(&mut self) {
        for scene in self.scene_list.iter_mut() {
            scene.change_language();
        }
    }
}

/// シングルトンで SceneManager を保持するため
pub struct WrappedSceneManager {
    scene_manager: Option<SceneManager>,
}
impl WrappedSceneManager {
    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    pub fn get(&mut self) -> &SceneManager {
        if self.scene_manager.is_none() {
            self.scene_manager = Some(SceneManager::default());
        }
        self.scene_manager.as_ref().unwrap()
    }

    // mut 版
    pub fn get_mut(&mut self) -> &mut SceneManager {
        if self.scene_manager.is_none() {
            self.scene_manager = Some(SceneManager::default());
        }
        self.scene_manager.as_mut().unwrap()
    }
}
static mut _SCENE_MANAGER: WrappedSceneManager = WrappedSceneManager {
    scene_manager: None,
};
#[allow(non_snake_case)]
pub fn SCENE_MANAGER() -> &'static mut WrappedSceneManager {
    unsafe { &mut _SCENE_MANAGER }
}

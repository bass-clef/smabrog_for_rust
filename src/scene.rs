
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


#[derive(Copy, Clone)]
pub enum ColorFormat {
    NONE = 0, GRAY = 1,
    RGB = 3, RGBA = 4,
}


/// シーン判定汎用クラス
pub struct SceneJudgment {
    color_image: core::Mat,
    mask_image: Option<core::Mat>,
    trans_mask_image: Option<core::Mat>,
    image_size: Option<core::Rect>,
    judgment_type: ColorFormat,
    pub prev_match_ratio: f64,
    pub prev_match_point: core::Point,
    border_match_ratio: f64,
}
impl Default for SceneJudgment {
    fn default() -> Self {
        Self {
            color_image: core::Mat::default(),
            mask_image: None,
            trans_mask_image: None,
            image_size: None,
            judgment_type: ColorFormat::RGB,
            border_match_ratio: 0.98,
            prev_match_ratio: 0f64,
            prev_match_point: Default::default(),
        }
    }
}
impl SceneJudgment {
    // 言語によって読み込むファイルを変えて作成する
    fn news_with_lang<T>(new_func: T, name: &str) -> Self
    where T: Fn(core::Mat, Option<core::Mat>) -> opencv::Result<Self>
    {
        use crate::resource::LANG_LOADER;
        use i18n_embed::LanguageLoader;

        let lang = LANG_LOADER().get().current_language().language.clone();
        let path = format!("resource/{}_{}", lang.as_str(), name);

        new_func(
            imgcodecs::imread(&format!("{}_color.png", path), imgcodecs::IMREAD_UNCHANGED).unwrap(),
            Some(imgcodecs::imread(&format!("{}_mask.png", path), imgcodecs::IMREAD_UNCHANGED).unwrap())
        ).unwrap()
    }

    fn new_gray_with_lang(name: &str) -> Self { Self::news_with_lang(Self::new_gray, name) }
    fn new_with_lang(name: &str) -> Self { Self::news_with_lang(Self::new, name) }
    // fn new_trans_with_lang(name: &str) -> Self { Self::news_with_lang(Self::new_trans, name) }

    /// color_format に {hoge}_image を強制して、一致させるシーン
    fn new_color_format(color_image: core::Mat, mask_image: Option<core::Mat>, color_format: ColorFormat) -> opencv::Result<Self> {
        let mut converted_color_image = core::Mat::default();

        // 強制で color_format にする
        utils::cvt_color_to(&color_image, &mut converted_color_image, color_format as i32)?;

        let converted_mask_image = match &mask_image {
            Some(v) => {
                let mut converted_mask_image = core::Mat::default();
                utils::cvt_color_to(&v, &mut converted_mask_image, color_format as i32)?;

                Some(converted_mask_image)
            },
            None => None,
        };

        Ok(Self {
            color_image: converted_color_image, mask_image: converted_mask_image,
            trans_mask_image: None,
            judgment_type: color_format,
            ..Default::default()
        })
    }
    /// 白黒画像と一致するシーン
    fn new_gray(color_image: core::Mat, mask_image: Option<core::Mat>) -> opencv::Result<Self> {
        Self::new_color_format(color_image, mask_image, ColorFormat::GRAY)
    }
    /// 普通のRGB画像と一致するシーン
    fn new(color_image: core::Mat, mask_image: Option<core::Mat>) -> opencv::Result<Self> {
        Self::new_color_format(color_image, mask_image, ColorFormat::RGB)
    }
    /// 透過画像と一致するシーン
    fn new_trans(color_image: core::Mat, mask_image: Option<core::Mat>) -> opencv::Result<Self> {
        // 他のメンバを透過に変換しておく
        let mut converted_color_image = core::Mat::default();
        utils::cvt_color_to(&color_image, &mut converted_color_image, ColorFormat::RGBA as i32)?;

        // 透過画像の場合は予め透過マスクを作成する
        let mut converted_mask_image = core::Mat::default();
        let mut trans_mask_image = core::Mat::default();
        match &mask_image {
            Some(v) => {
                // 強制で RGBA にする
                utils::cvt_color_to(&v, &mut trans_mask_image, ColorFormat::RGBA as i32)?;
                utils::cvt_color_to(&v, &mut converted_mask_image, ColorFormat::RGBA as i32)?;
            },
            None => {
                utils::make_trans_mask_from_noalpha(&color_image, &mut trans_mask_image)?;
            },
        }

        Ok(Self {
            color_image: converted_color_image, mask_image: Some(converted_mask_image),
            trans_mask_image: Some(trans_mask_image),
            judgment_type: ColorFormat::RGBA,
            ..Default::default()
        })
    }

    /// 一致率の上限の設定 [default = 0.99]
    fn set_border(mut self, border_match_ratio: f64) -> Self {
        self.border_match_ratio = border_match_ratio;

        self
    }
    
    /// 検出領域の設定 [default = full]
    fn set_size(mut self, new_size: core::Rect) -> Self {
        self.image_size = Some(new_size);

        self.color_image = core::Mat::roi(&self.color_image.clone(), new_size).unwrap();

        if let Some(trans_mask_image) = self.trans_mask_image {
            self.trans_mask_image = Some(core::Mat::roi(&trans_mask_image, new_size).unwrap());
        }
        if let Some(mask_image) = self.mask_image {
            self.mask_image = Some(core::Mat::roi(&mask_image, new_size).unwrap());
        }

        self
    }

    /// キャプチャされた画像とシーンとをテンプレートマッチングして、一致した確率と位置を返す
    async fn match_captured_scene(&mut self, captured_image: &core::Mat) {
        let mut result = core::Mat::default();
        let mut converted_captured_image = core::Mat::default();
        if let Some(image_size) = self.image_size {
            utils::cvt_color_to(
                &core::Mat::roi(&captured_image, image_size).unwrap(),
                &mut converted_captured_image, self.judgment_type as i32
            ).unwrap();
        } else {
            utils::cvt_color_to(captured_image, &mut converted_captured_image, self.judgment_type as i32).unwrap();
        }

        match self.judgment_type {
            ColorFormat::NONE => (),
            ColorFormat::RGB | ColorFormat::GRAY => {
                // [2値 | RGB]画像はマスクがあれば and かけて、ないならテンプレートマッチング
                // None の場合は converted_captured_image はコピーされた状態だけでよい
                if let Some(mask_image) = &self.mask_image {
                    // captured_image を mask_image で篩いにかけて,無駄な部分を削ぐ
                    // どうでもいいけどソースをみてそれに上書きしてほしいとき、同じ変数を指定できないの欠陥すぎね？？？(これが安全なメモリ管理か、、、。)
                    let mut temp_captured_image = converted_captured_image.clone();

                    match core::bitwise_and(&converted_captured_image, &mask_image,
                        &mut temp_captured_image, &core::no_array())
                    {
                        Ok(_) => (),
                        Err(_e) => {
                            // サイズの違いや色深度の違いでエラーになることがあるけど、マスクがかけられなかった
                            return;
                        },
                    }
                    converted_captured_image = temp_captured_image;
                }

                imgproc::match_template(&converted_captured_image, &self.color_image, &mut result,
                    imgproc::TM_CCOEFF_NORMED, &core::no_array()).unwrap();
            },
            ColorFormat::RGBA => {
                // 透過画像の場合は普通に trans_mask 付きでテンプレートマッチング
                // 透過画像の時はそもそも None の状態になることはない
                if let Some(trans_mask_image) = &self.trans_mask_image {
                    imgproc::match_template(&converted_captured_image, &self.color_image, &mut result,
                        imgproc::TM_CCORR_NORMED, &trans_mask_image).unwrap();
                }
            },
        };

        core::patch_na_ns(&mut result, -0.0).unwrap();
        utils::patch_inf_ns(&mut result, -0.0).unwrap();

        core::min_max_loc(&result,
            None, Some(&mut self.prev_match_ratio),
            None, Some(&mut self.prev_match_point),
            &core::no_array()
        ).unwrap();
    }

    /// 前回のテンプレートマッチングで大体一致しているか
    pub fn is_near_match(&self) -> bool {
        self.border_match_ratio <= self.prev_match_ratio
    }

    /// is_near_match が確定するのに必要な確率
    pub fn get_border_match_ratio(&self) -> f64 {
        self.border_match_ratio
    }
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

/// 状態不明のシーン
#[derive(Default)]
struct UnknownScene {}
impl SceneTrait for UnknownScene {
    fn get_id(&self) -> i32 { SceneList::Unknown as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> { None }

    // 状態不明は他から遷移する、もしくは最初のシーンなので, 自身ではならない, 他に移らない, 録画しない,, データ検出しない
    fn continue_match(&self, _now_scene: SceneList) -> bool { false }
    fn is_scene(&mut self, _capture_image: &core::Mat, _smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> { Ok(false) }
    fn to_scene(&self, now_scene: SceneList) -> SceneList { now_scene }
    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}

/// 読込中のシーン
pub struct LoadingScene {
    scene_judgment: SceneJudgment,
}
impl Default for LoadingScene {
    fn default() -> Self {
        Self {
            scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/loading_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/loading_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_border(0.95)
            .set_size(core::Rect{    // 参照される回数が多いので matchTemplate する大きさ減らす
                x:0, y:100, width:640, height: 260
            }),
        }
    }
}
impl SceneTrait for LoadingScene {
    fn get_id(&self) -> i32 { SceneList::Loading as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> { Some(&self.scene_judgment) }

    // 読込中の画面はどのシーンでも検出しうる
    fn continue_match(&self, _now_scene: SceneList) -> bool { true }

    fn is_scene(&mut self, capture_image: &core::Mat, _smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        async_std::task::block_on(async {
            self.scene_judgment.match_captured_scene(&capture_image).await;
        });

        Ok(self.scene_judgment.is_near_match())
    }

    // このシーンからは複数の遷移があるので、現状維持
    fn to_scene(&self, now_scene: SceneList) -> SceneList { now_scene }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }

    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}

/// ダイアログが表示されているシーン
/// 突然の回線切断とか、連続して試合をするとき、録画のYボタンを押したとき、など
struct DialogScene {
    scene_judgment: SceneJudgment,
}
impl Default for DialogScene {
    fn default() -> Self {
        Self {
            scene_judgment: SceneJudgment::new(
                imgcodecs::imread("resource/battle_retry_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/battle_retry_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_border(0.98)
        }
    }
}
impl SceneTrait for DialogScene {
    fn get_id(&self) -> i32 { SceneList::Dialog as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> { Some(&self.scene_judgment) }
    
    // 回線切断などでどのシーンでも検出しうるけど、それらは ReadyToFight を通るので、Result 後のみでいい
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::Result => true,
            _ => false,
        }
    }
    
    fn is_scene(&mut self, capture_image: &core::Mat, _smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        async_std::task::block_on(async {
            self.scene_judgment.match_captured_scene(&capture_image).await;
        });

        Ok(self.scene_judgment.is_near_match())
    }

    // このシーンからは複数の遷移があるけど、表示された後は常に最初に戻る
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::Unknown }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }

    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}

/// Ready to Fight が表示されているシーン]
/// スマブラがちゃんとキャプチャされているかで使用
pub struct ReadyToFightScene {
    pub grad_scene_judgment: SceneJudgment,
    pub red_scene_judgment: SceneJudgment,
    pub scene_judgment_skip_wait: i32,
}
impl Default for ReadyToFightScene {
    fn default() -> Self { Self::new_gray() }
}
impl SceneTrait for ReadyToFightScene {
    fn get_id(&self) -> i32 { SceneList::ReadyToFight as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> {
        // 高い方を返す
        if self.red_scene_judgment.prev_match_ratio < self.grad_scene_judgment.prev_match_ratio {
            return Some(&self.grad_scene_judgment);
        }

        Some(&self.red_scene_judgment)
    }

    // 回線切断などの原因で最初に戻ることは常にあるので gray match だし常に判定だけしておく
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::ReadyToFight => false,
            _ => true,
        }
    }
    
    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        if let Some(data) = smashbros_data.as_ref() {
            if data.is_playing_battle() {
                // 試合中の検出は数回に一回で十分
                if 0 < self.scene_judgment_skip_wait {
                    self.scene_judgment_skip_wait -= 1;
                    return Ok(false)
                }

                self.scene_judgment_skip_wait = 10;
            } else if 0 < self.scene_judgment_skip_wait {
                self.scene_judgment_skip_wait = 0;
            }
        }

        // 多分 grad版 ReadyToFight のほうが多いので先にする
        // (grad:カーソルが on_cursor の状態, red: わざとカーソルを READY to FIGHT からずらしている状態)
        async_std::task::block_on(async {
            self.grad_scene_judgment.match_captured_scene(&capture_image).await;
            if self.grad_scene_judgment.is_near_match() {
                return; // async-function
            }

            self.red_scene_judgment.match_captured_scene(&capture_image).await;
        });
        
        Ok( self.grad_scene_judgment.is_near_match() || self.red_scene_judgment.is_near_match() )
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::ReadyToFight }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}
impl ReadyToFightScene {
    pub fn new_gray() -> Self {
        Self {
            grad_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/ready_to_fight_color_0.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/ready_to_fight_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_size(core::Rect{    // 参照される回数が多いので matchTemplate する大きさ減らす
                x:0, y:0, width:640, height: 180
            }),
            red_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/ready_to_fight_color_1.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/ready_to_fight_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_size(core::Rect{
                x:0, y:0, width:640, height: 180
            }),
            scene_judgment_skip_wait: 0,
        }
    }

    pub fn new_trans() -> Self {
        Self {
            grad_scene_judgment: SceneJudgment::new_trans(
                imgcodecs::imread("resource/ready_to_fight_color_0.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/ready_to_fight_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            red_scene_judgment: SceneJudgment::new_trans(
                imgcodecs::imread("resource/ready_to_fight_color_1.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/ready_to_fight_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            scene_judgment_skip_wait: 0,
        }
    }
}

/// マッチング中の画面 (CPUと戦えるあの画面)
/// save: プレイヤー人数(2p, 4p)
struct MatchingScene {
    scene_judgment: SceneJudgment,
    scene_judgment_with4: SceneJudgment,
    scene_judgment_ooo_tournament: SceneJudgment,
    scene_judgment_smash_tournament: SceneJudgment,
}
impl Default for MatchingScene {
    fn default() -> Self {
        Self {
            scene_judgment: SceneJudgment::new_with_lang("ready_ok")
                .set_border(0.92)
                .set_size(core::Rect{    // 参照される回数が多いので matchTemplate する大きさ減らす
                    x:0, y:270, width:320, height: 90
                }),
            scene_judgment_with4: SceneJudgment::new_with_lang("with_4_battle")
                .set_size(core::Rect{
                    x:0, y:270, width:640, height: 90
                }),
            scene_judgment_ooo_tournament: SceneJudgment::new(
                    imgcodecs::imread("resource/ooo_tournament_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/tournament_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                )
                .unwrap()
                .set_border(0.95)
                .set_size(core::Rect{
                    x:0, y:0, width:640, height: 30
                }),
            scene_judgment_smash_tournament: SceneJudgment::new(
                    imgcodecs::imread("resource/smash_tournament_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/tournament_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                )
                .unwrap()
                .set_border(0.95)
                .set_size(core::Rect{
                    x:0, y:0, width:640, height: 30
                }),
        }
    }
}
impl SceneTrait for MatchingScene {
    fn change_language(&mut self) { *self = Self::default(); }
    fn get_id(&self) -> i32 { SceneList::Matching as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> {
        // 一番高い確率のものを返す
        let mut most_ratio = self.scene_judgment.prev_match_ratio;
        let mut most_scene_judgment = &self.scene_judgment;

        if most_ratio < self.scene_judgment_with4.prev_match_ratio {
            most_ratio = self.scene_judgment_with4.prev_match_ratio;
            most_scene_judgment = &self.scene_judgment_with4;
        }
        if most_ratio < self.scene_judgment_ooo_tournament.prev_match_ratio {
            most_scene_judgment = &self.scene_judgment_ooo_tournament;
        }

        Some(most_scene_judgment)
    }
    
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::Unknown | SceneList::ReadyToFight | SceneList::GameEnd | SceneList::Result => true,
            _ => false,
        }
    }

    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        // 多分 1on1 のほうが多いけど with 4 のほうにも 1on1 が一致するので with4 を先にする
        async_std::task::block_on(async {
            // self.scene_judgment_smash_tournament.match_captured_scene(&capture_image).await;
            // if self.scene_judgment_smash_tournament.is_near_match() {
            //     return;
            // }

            self.scene_judgment_ooo_tournament.match_captured_scene(&capture_image).await;
            if self.scene_judgment_ooo_tournament.is_near_match() {
                return;
            }

            self.scene_judgment_with4.match_captured_scene(&capture_image).await;
            if self.scene_judgment_with4.is_near_match() {
                return;
            }

            self.scene_judgment.match_captured_scene(&capture_image).await;
        });

        if let Some(smashbros_data) = smashbros_data {
            if self.scene_judgment.is_near_match() {
                smashbros_data.initialize_battle(2, true);
                return Ok(true);
            } else if self.scene_judgment_with4.is_near_match() {
                smashbros_data.initialize_battle(4, true);
                return Ok(true);
            } else if self.scene_judgment_ooo_tournament.is_near_match() {
                smashbros_data.initialize_battle(2, true);
                smashbros_data.set_rule(BattleRule::Tournament);
                return Ok(true);
            } else if self.scene_judgment_smash_tournament.is_near_match() {
                smashbros_data.initialize_battle(4, true);
                smashbros_data.set_rule(BattleRule::Tournament);
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::Matching }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}

/// キャラクターが大きく表示されてる画面
/// save: キャラクター名, ルール名, 取れるなら[時間,ストック,HP]
struct HamVsSpamScene {
    vs_scene_judgment: SceneJudgment,
    rule_stock_scene_judgment: SceneJudgment,
    rule_time_scene_judgment: SceneJudgment,
    rule_stamina_scene_judgment: SceneJudgment,
    buffer: CaptureFrameStore,
}
impl Default for HamVsSpamScene {
    fn default() -> Self {
        Self {
            vs_scene_judgment: SceneJudgment::new_with_lang("vs"),
            rule_stock_scene_judgment: SceneJudgment::new(
                    imgcodecs::imread("resource/rule_stock_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/rule_stock_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.985),
            rule_time_scene_judgment: SceneJudgment::new(
                    imgcodecs::imread("resource/rule_time_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/rule_time_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.985),
            rule_stamina_scene_judgment: SceneJudgment::new(
                    imgcodecs::imread("resource/rule_hp_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/rule_hp_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.985),
            buffer: CaptureFrameStore::default(),
        }
    }
}
impl SceneTrait for HamVsSpamScene {
    fn change_language(&mut self) { *self = Self::default(); }
    fn get_id(&self) -> i32 { SceneList::HamVsSpam as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> { Some(&self.vs_scene_judgment) }
    
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::Matching => true,
            _ => false,
        }
    }

    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        if let Some(smashbros_data) = smashbros_data.as_ref() {
            if smashbros_data.all_decided_character_name() {
                // すべてのプレイヤーが確定している場合は判定すら行わない (matchTemaplte は処理コストが高い)
                return Ok(false);
            }
        }

        async_std::task::block_on(async {
            self.vs_scene_judgment.match_captured_scene(&capture_image).await;
        });

        if self.vs_scene_judgment.is_near_match() {
            imgcodecs::imwrite("ham_vs_spam.png", capture_image, &core::Vector::from(vec![]))?;
            self.buffer.start_recoding_by_time(std::time::Duration::from_millis(2500));
            self.buffer.recoding_frame(capture_image)?;
        }
        Ok(self.vs_scene_judgment.is_near_match())
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::HamVsSpam }

    fn recoding_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<()> { self.buffer.recoding_frame(capture_image) }
    fn is_recoded(&self) -> bool { self.buffer.is_filled() }

    /// save: キャラクターの種類, ルール(time | stock | stamina), 時間
    fn detect_data(&mut self, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let Self {
            rule_stock_scene_judgment,
            rule_time_scene_judgment,
            rule_stamina_scene_judgment,
            buffer,
            ..
        } = self;

        buffer.replay_frame(|frame| {
            Self::captured_rules(&frame, smashbros_data, rule_stock_scene_judgment, rule_time_scene_judgment, rule_stamina_scene_judgment)?;
            Self::captured_character_name(&frame, smashbros_data)?;

            Ok(false)
        })?;

        Ok(())
    }
}
impl HamVsSpamScene {
    pub fn captured_character_name(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        if smashbros_data.all_decided_character_name() {
            return Ok(true);
        }

        use regex::Regex;
        let mut gray_capture_image = core::Mat::default();
        utils::cvt_color_to(capture_image, &mut gray_capture_image, ColorFormat::GRAY as i32)?;

        // 近似白黒処理して
        let mut temp_capture_image = core::Mat::default();
        let mut work_capture_image = core::Mat::default();
        imgproc::threshold(&gray_capture_image, &mut work_capture_image, 200.0, 255.0, imgproc::THRESH_BINARY)?;
        core::bitwise_not(&work_capture_image, &mut temp_capture_image, &core::no_array())?;

        // プレイヤー毎の位置で処理する
        let (width, height) = (capture_image.cols(), capture_image.rows());
        let player_area_width = width / smashbros_data.get_player_count();
        let re = Regex::new(r"\s*(\w+)\s*").unwrap();
        let mut skip_count = 0;
        for player_count in 0..smashbros_data.get_player_count() {
            if smashbros_data.is_decided_character_name(player_count) {
                // 既にプレイヤーキャラクターが確定しているならスキップ
                skip_count += 1;
                continue;
            }
            // 高さそんなにいらないので適当に小さくする
            let player_name_area = core::Rect {
                x: player_area_width*player_count +30, y: 0,        // 30:{N}P のプレイヤー表示の幅
                width: player_area_width -20 -30, height: height/7  // 10:稲妻が処理後に黒四角形になって文字領域として誤認されるのを防ぐため
            };
            let mut name_area_image = core::Mat::roi(&temp_capture_image, player_name_area)?;
            let gray_name_area_image = core::Mat::roi(&work_capture_image, player_name_area)?;

            // 輪郭捕捉して
            let name_contour_image = utils::trimming_any_rect(
                &mut name_area_image, &gray_name_area_image, Some(5), None, None, false, None)?;
            utils::cvt_color_to(&name_contour_image, &mut name_area_image, ColorFormat::RGB as i32)?;

            // tesseract でキャラ名取得して, 余計な文字を排除
            let text = &async_std::task::block_on(utils::run_ocr_with_upper_alpha(&name_area_image)).unwrap();
            if let Some(caps) = re.captures( text ) {
                smashbros_data.guess_character_name( player_count, String::from(&caps[1]) );
            }
        }

        Ok(smashbros_data.get_player_count() == skip_count)
    }

    pub fn captured_rules(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, rule_stock_scene_judgment: &mut SceneJudgment, rule_time_scene_judgment: &mut SceneJudgment, rule_stamina_scene_judgment: &mut SceneJudgment) -> opencv::Result<bool> {
        if smashbros_data.get_rule() == BattleRule::Tournament {
            return Ok(false);
        }

        if !smashbros_data.is_decided_rule() {
            async_std::task::block_on(async {
                // ストック制と検出(1on1: これがデフォルトで一番多いルール)
                rule_stock_scene_judgment.match_captured_scene(capture_image).await;
                if rule_stock_scene_judgment.is_near_match() {
                    smashbros_data.set_rule(BattleRule::Stock);
                    log::info!("rule: stock: {:2.3}%", rule_stock_scene_judgment.prev_match_ratio);
                    return;
                }

                rule_time_scene_judgment.match_captured_scene(capture_image).await;
                if rule_time_scene_judgment.is_near_match() {
                    smashbros_data.set_rule(BattleRule::Time);
                    log::info!("rule: time {:2.3}%", rule_time_scene_judgment.prev_match_ratio);
                    return;
                }

                rule_stamina_scene_judgment.match_captured_scene(capture_image).await;
                if rule_stamina_scene_judgment.is_near_match() {
                    smashbros_data.set_rule(BattleRule::Stamina);
                    log::info!("rule: stamina {:2.3}%", rule_stamina_scene_judgment.prev_match_ratio);
                    return;
                }
            });
        }

        // 各ルール条件の検出
        let mut time_area: Option<core::Mat> = None;
        let mut sec_time_area: Option<core::Mat> = None;
        let mut stock_area: Option<core::Mat> = None;
        let mut hp_area: Option<core::Mat> = None;
        match smashbros_data.get_rule() {
            BattleRule::Time => {
                // Time   : 時間制限あり[2,2:30,3], ストック数は上限なしの昇順, HPはバースト毎に0%に初期化
                time_area = Some(core::Mat::roi( capture_image, core::Rect {x:313, y:332, width:10, height:20})? );
                sec_time_area = Some(core::Mat::roi( capture_image, core::Rect {x:325, y:332, width:18, height:20})? );
            },
            BattleRule::Stock => {
                // Stock  : 時間制限あり[3,4,5,6,7], ストック数は上限[1,2,3]の降順, HPはバースト毎に0%に初期化
                time_area = Some(core::Mat::roi( capture_image, core::Rect {x:274, y:332, width:11, height:20})? );
                stock_area = Some(core::Mat::roi( capture_image, core::Rect {x:358, y:332, width:11, height:20})? );
            },
            BattleRule::Stamina => {
                // Stamina: 時間制限あり[3,4,5,6,7], ストック数は上限[1,2,3]の降順, HPは上限[100,150,200,250,300]の降順
                time_area = Some(core::Mat::roi( capture_image, core::Rect {x:241, y:332, width:11, height:20})? );
                stock_area = Some(core::Mat::roi( capture_image, core::Rect {x:324, y:332, width:11, height:20})? );
                hp_area = Some(core::Mat::roi( capture_image, core::Rect {x:380, y:332, width:18, height:20})? );
            },
            _ => ()
        }

        if let Some(mut sec_time_area) = sec_time_area {
            Self::captured_time_with_sec(&mut time_area.unwrap(), &mut sec_time_area, smashbros_data)?;
        } else if let Some(mut time_area) = time_area {
            Self::captured_time(&mut time_area, smashbros_data)?;
        }
        if let Some(mut stock_area) = stock_area {
            Self::captured_stock(&mut stock_area, smashbros_data)?;
        }
        if let Some(mut hp_area) = hp_area {
            Self::captured_stamina(&mut hp_area, smashbros_data)?;
        }

        // ストック と 制限時間 が下から上がってくる演出を出していて、誤検出しやすいので, frame を全部処理する
        Ok(smashbros_data.is_decided_rule_all_clause())
    }

    pub fn captured_time(capture_image: &mut core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let time = match Self::captured_convert_number(capture_image, r"\s*(\d)\s*", Some("34567"), true) {
            Ok(time) => time.parse::<u64>().unwrap_or(0) * 60,
            Err(_) => 0,
        };

        smashbros_data.guess_max_time(time);

        Ok(())
    }

    pub fn captured_time_with_sec(capture_image: &mut core::Mat, sec_capture_image: &mut core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let time = match Self::captured_convert_number(capture_image, r"\s*(\d)\s*", Some("23"), true) {
            Ok(time) => time.parse::<u64>().unwrap_or(0) * 60,
            Err(_) => 0,
        };
        let sec_time = match Self::captured_convert_number(sec_capture_image, r"\s*(\d+)\s*", Some("03"), false) {
            Ok(sec_time) => sec_time.parse::<u64>().unwrap_or(0),
            Err(_) => 0,
        };

        smashbros_data.guess_max_time(time + sec_time);

        Ok(())
    }

    pub fn captured_stock(capture_image: &mut core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let stock: i32 = match Self::captured_convert_number(capture_image, r"\s*(\d)\s*", Some("123"), true) {
            Ok(stock) => stock.parse().unwrap_or(-1),
            Err(_) => 0,
        };

        for player_number in 0..smashbros_data.get_player_count() {
            smashbros_data.guess_max_stock(player_number, stock);
        }

        Ok(())
    }

    pub fn captured_stamina(capture_image: &mut core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let hp = match Self::captured_convert_number(capture_image, r"\s*(\d+)\s*", Some("01235"), false) {
            Ok(hp) => hp.parse().unwrap_or(-1) * 10,
            Err(_) => 0,
        };

        for player_number in 0..smashbros_data.get_player_count() {
            smashbros_data.guess_max_hp(player_number, hp);
        }


        Ok(())
    }

    /// capture_image から検出した文字列を regex_pattern で正規表現にかけて文字列(数値)にして返す
    pub fn captured_convert_number(capture_image: &mut core::Mat, regex_pattern: &str, valid_string: Option<&str>, is_single_char: bool) -> opencv::Result<String> {
        use regex::Regex;
        // 近似白黒処理して
        let mut gray_capture_image = core::Mat::default();
        imgproc::threshold(capture_image, &mut gray_capture_image, 100.0, 255.0, imgproc::THRESH_BINARY)?;
        let mut work_capture_image = core::Mat::default();
        utils::cvt_color_to(&gray_capture_image, &mut work_capture_image, ColorFormat::GRAY as i32)?;

        // 白黒反転して
        core::bitwise_not(&work_capture_image, &mut gray_capture_image, &core::no_array())?;

        // tesseract で文字(数値)を取得して, 余計な文字を排除
        let text = &async_std::task::block_on(utils::run_ocr_with_number(&gray_capture_image, valid_string, is_single_char)).unwrap().to_string();
        let re = Regex::new(regex_pattern).unwrap();
        if let Some(caps) = re.captures( text ) {
            return Ok( caps[1].to_string() );
        }

        Err(opencv::Error::new( 0, "not found anything. from captured_convert_number".to_string() ))
    }
}

/// 試合開始の検出
pub struct GameStartScene {
    scene_judgment: SceneJudgment,
    is_scene: bool,
}
impl Default for GameStartScene {
    fn default() -> Self {
        Self {
            scene_judgment: SceneJudgment::new_trans(
                    imgcodecs::imread("resource/battle_time_zero_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/battle_time_zero_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.8),
            is_scene: false,
        }
    }
}
impl SceneTrait for GameStartScene {
    fn get_id(&self) -> i32 { SceneList::GameStart as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> { Some(&self.scene_judgment) }
    
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::HamVsSpam => true,
            _ => false,
        }
    }

    // このシーンだけ検出が厳しい。
    // "GO"でなくて 時間の 00.00 で検出するという ("GO"はエフェクトかかりすぎて検出しづらかった
    // ラグとかある状況も予想されるので、00.00 が検出できたら"GO"とでていなくとも次に遷移する
    // 右上の 00.00 が表示されている場所に ある程度の確率で検出してればよしとする
    // (背景がステージによって全然違うのでマスク処理するのが難しい)
    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        let time_zero_area = core::Mat::roi( capture_image, core::Rect {x:566, y:11, width:62, height:27}).unwrap();
        async_std::task::block_on(async {
            self.scene_judgment.match_captured_scene(&time_zero_area).await;
        });

        if self.scene_judgment.is_near_match() {
            self.is_scene = true;
            if let Some(smashbros_data) = smashbros_data {
                self.captured_count_down(capture_image, smashbros_data)?;
            }
        } else if self.is_scene {
            // N:00 の状態はまだ始まっていないので、違ってくる時に次のシーンに遷移する
            self.is_scene = false;
            return Ok(true);
        }

        Ok(false)
    }

    // now_scene が GameStart になることはない("GO"を検出した時はもう GamePlaying であるため)
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GamePlaying }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}
impl GameStartScene {
    // カウントダウン が検出されているフレームの処理
    fn captured_count_down(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        self.captured_bgm_name(capture_image, smashbros_data)?;

        Ok(())
    }

    // BGM が検出されているフレームを処理
    fn captured_bgm_name(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let mut bgm_capture_image = core::Mat::roi(capture_image, core::Rect::new(18, 30, 240, 18))?;

        // 近似白黒処理して
        let mut gray_capture_image = core::Mat::default();
        imgproc::threshold(&bgm_capture_image, &mut gray_capture_image, 150.0, 255.0, imgproc::THRESH_BINARY)?;
        let mut work_capture_image = core::Mat::default();
        utils::cvt_color_to(&gray_capture_image, &mut work_capture_image, ColorFormat::GRAY as i32)?;

        // opencv::highgui::imshow("gray_capture_image", &gray_capture_image)?;

        // 輪郭捕捉して
        let work_capture_image = utils::trimming_any_rect(
            &mut bgm_capture_image, &work_capture_image, Some(5), Some(0.0), None, true, Some(core::Scalar::new(128.0, 128.0, 128.0, 0.0)))?;

        // 白黒反転して
        core::bitwise_not(&work_capture_image, &mut gray_capture_image, &core::no_array())?;
        let mut work_capture_image = core::Mat::default();
        utils::cvt_color_to(&gray_capture_image, &mut work_capture_image, ColorFormat::GRAY as i32)?;

        // opencv::highgui::imshow("bgm_capture_image", &work_capture_image)?;

        // tesseract で文字列を取得して, 余計な文字を排除
        let bgm_text = &async_std::task::block_on(utils::run_ocr_with_japanese(&work_capture_image)).unwrap().to_string();
        if bgm_text.is_empty() {
            return Ok(());
        }
        let bgm_text = bgm_text.replace(" ", "");

        smashbros_data.guess_bgm_name(bgm_text);

        Ok(())
    }
}

/// 試合中の検出
/// save: プレイヤー毎のストック(デカ[N - N]の画面の{N})
struct GamePlayingScene {
    stock_black_scene_judgment: SceneJudgment,
    stock_white_scene_judgment: SceneJudgment,
    buffer: CaptureFrameStore,
    stock_number_mask: core::Mat,
}
impl Default for GamePlayingScene {
    fn default() -> Self {
        Self {
            stock_black_scene_judgment: SceneJudgment::new_gray(
                    imgcodecs::imread("resource/stock_hyphen_color_black.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/stock_hyphen_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_size(core::Rect{    // 参照される回数が多いので matchTemplate する大きさ減らす
                    x:0, y:100, width:640, height: 100
                })
                .set_border(0.95),
            stock_white_scene_judgment: SceneJudgment::new_gray(
                    imgcodecs::imread("resource/stock_hyphen_color_white.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/stock_hyphen_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_size(core::Rect{
                    x:0, y:100, width:640, height: 100
                })
                .set_border(0.95),
            buffer: CaptureFrameStore::default(),
            stock_number_mask: imgcodecs::imread("resource/stock_number_mask.png", imgcodecs::IMREAD_GRAYSCALE).unwrap()
        }
    }
}
impl SceneTrait for GamePlayingScene {
    fn get_id(&self) -> i32 { SceneList::GamePlaying as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> {
        // 高い方を返す
        if self.stock_black_scene_judgment.prev_match_ratio < self.stock_white_scene_judgment.prev_match_ratio {
            return Some(&self.stock_white_scene_judgment);
        }

        Some(&self.stock_black_scene_judgment)
    }
    
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::GamePlaying => true,
            _ => false,
        }
    }

    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        let smashbros_data = match smashbros_data {
            Some(smashbros_data) => smashbros_data,
            None => return Ok(false),
        };

        match smashbros_data.get_player_count() {
            2 => self.game_playing_with_2(capture_image, smashbros_data),
            4 => self.game_playing_with_4(capture_image, smashbros_data),
            _ => Ok(false) // TODO?: 8 人対戦とか?
        }
    }

    // このシーンは [GameEnd] が検出されるまで待つ(つまり現状維持)
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GamePlaying }

    fn recoding_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<()> { self.buffer.recoding_frame(capture_image) }
    fn is_recoded(&self) -> bool { self.buffer.is_filled() }
    fn detect_data(&mut self, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let stock_number_mask = &self.stock_number_mask;
        self.buffer.replay_frame(|frame| {
            Self::captured_stock_number(&frame, smashbros_data, stock_number_mask)
        })?;
        Ok(())
    }
}
impl GamePlayingScene {
    // 1 on 1
    fn game_playing_with_2(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        match smashbros_data.get_rule() {
            BattleRule::Stock | BattleRule::Stamina => {
                self.stock_scene_judgment(capture_image, smashbros_data)?;
            },
            _ => (),
        }
        Ok(false)
    }
    // smash
    fn game_playing_with_4(&mut self, _capture_image: &core::Mat, _smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        Ok(false)
    }

    // ストックを検出
    fn stock_scene_judgment(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        if smashbros_data.all_decided_stock() {
            // すべてのプレイヤーが確定している場合は判定すら行わない (matchTemaplte は処理コストが高い)
            return Ok(false);
        }

        if !self.buffer.is_recoding_started() {
            async_std::task::block_on(async {
                self.stock_black_scene_judgment.match_captured_scene(&capture_image).await;
                if self.stock_black_scene_judgment.is_near_match() {
                    return; // async-function
                }
                
                self.stock_white_scene_judgment.match_captured_scene(&capture_image).await;
            });
        }

        if self.stock_black_scene_judgment.is_near_match() || self.stock_white_scene_judgment.is_near_match() {
            self.buffer.start_recoding_by_time(std::time::Duration::from_secs(1));
            self.buffer.recoding_frame(capture_image)?;
        }

        Ok(false)
    }

    // ストックが検出されているフレームを処理
    pub fn captured_stock_number(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, stock_number_mask: &core::Mat) -> opencv::Result<bool> {
        use regex::Regex;
        // ストックの位置を切り取って
        let mut temp_capture_image = core::Mat::default();
        let mut gray_number_area_image = core::Mat::default();
        utils::cvt_color_to(&capture_image, &mut gray_number_area_image, ColorFormat::GRAY as i32)?;
        core::bitwise_and(&gray_number_area_image, stock_number_mask, &mut temp_capture_image, &core::no_array())?;

        // 近似白黒処理して
        let mut work_capture_image = core::Mat::default();
        imgproc::threshold(&temp_capture_image, &mut work_capture_image, 250.0, 255.0, imgproc::THRESH_BINARY)?;
        core::bitwise_and(&gray_number_area_image, &work_capture_image, &mut temp_capture_image, &core::no_array())?;
        core::bitwise_not(&temp_capture_image, &mut work_capture_image, &core::no_array())?;

        // プレイヤー毎に処理する
        let (width, height) = (capture_image.cols(), capture_image.rows());
        let player_area_width = width / smashbros_data.get_player_count();
        let re = Regex::new(r"\s*(\d)\s*").unwrap();
        let mut skip_count = 0;
        for player_number in 0..smashbros_data.get_player_count() {
            if smashbros_data.is_decided_stock(player_number) {
                // 既にプレイヤーのストックが確定しているならスキップ
                skip_count += 1;
                continue;
            }
            // 適当に小さくする
            let player_stock_area = core::Rect {
                x: player_area_width*player_number, y: height/4, width: player_area_width, height: height/2
            };
            let mut stock_area_image = core::Mat::roi(&work_capture_image, player_stock_area)?;
            let gray_stock_area_image = core::Mat::roi(&gray_number_area_image, player_stock_area)?;

            // 輪郭捕捉して
            let stock_contour_image = utils::trimming_any_rect(
                &mut stock_area_image, &gray_stock_area_image, Some(5), Some(1000.0), None, true, None)?;

            // tesseract で文字(数値)を取得して, 余計な文字を排除
            let number = &async_std::task::block_on(utils::run_ocr_with_number(&stock_contour_image, Some("123"), true)).unwrap().to_string();
            if let Some(caps) = re.captures( number ) {
                let number = (&caps[1]).parse().unwrap_or(-1);
                smashbros_data.guess_stock(player_number, number);
            }
        }

        Ok(smashbros_data.get_player_count() == skip_count)
    }
}

/// 試合終わりの検出 ("GAME SET" or "TIME UP")
struct GameEndScene {
    game_set_scene_judgment: SceneJudgment,
    time_up_scene_judgment: SceneJudgment,
}
impl Default for GameEndScene {
    fn default() -> Self {
        Self {
            game_set_scene_judgment: SceneJudgment::new_gray_with_lang("game_set")
                .set_border(0.85),
            time_up_scene_judgment: SceneJudgment::new_gray_with_lang("time_up")
                .set_border(0.85),
        }
    }
}
impl SceneTrait for GameEndScene {
    fn change_language(&mut self) { *self = Self::default(); }
    fn get_id(&self) -> i32 { SceneList::GameEnd as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> {
        // 高い方を返す
        if self.game_set_scene_judgment.prev_match_ratio < self.time_up_scene_judgment.prev_match_ratio {
            return Some(&self.time_up_scene_judgment);
        }

        Some(&self.game_set_scene_judgment)
    }
    
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::GamePlaying => true,
            _ => false,
        }
    }

    fn is_scene(&mut self, capture_image: &core::Mat, _smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        // おそらく、ストック制を選択してる人のほうが多い(もしくは時間より先に決着がつくことが多い)
        async_std::task::block_on(async {
            self.game_set_scene_judgment.match_captured_scene(&capture_image).await;
            if self.game_set_scene_judgment.is_near_match() {
                return; // async-function
            }

            self.time_up_scene_judgment.match_captured_scene(&capture_image).await;
        });

        Ok(self.game_set_scene_judgment.is_near_match() || self.time_up_scene_judgment.is_near_match())
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GameEnd }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}

/// 結果画面表示
/// save: プレイヤー毎の[戦闘力, 順位]
struct ResultScene {
    pub buffer: CaptureFrameStore,
    scene_judgment_list: Vec<SceneJudgment>,
    count_down_scene_judgment: SceneJudgment,
    retry_battle_scene_judgment: SceneJudgment,
    result_power_mask: core::Mat
}
impl Default for ResultScene {
    fn default() -> Self {
        let mut scene_judgment_list = vec![];
        for player_number in 1..=4 {
            let path = format!("resource/result_player_order_{}_", player_number);
            scene_judgment_list.push(
                SceneJudgment::new_trans(
                    imgcodecs::imread(&(path.clone() + "color.png"), imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread(&(path + "mask.png"), imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.985)
            );
        }

        Self {
            scene_judgment_list: scene_judgment_list,
            count_down_scene_judgment: SceneJudgment::new(
                imgcodecs::imread("resource/result_time_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/result_time_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_border(0.55),
            retry_battle_scene_judgment: SceneJudgment::new(
                imgcodecs::imread("resource/battle_retry_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/battle_retry_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            buffer: CaptureFrameStore::default(),
            result_power_mask: imgcodecs::imread("resource/result_power_mask.png", imgcodecs::IMREAD_GRAYSCALE).unwrap(),
        }
    }
}
impl SceneTrait for ResultScene {
    fn get_id(&self) -> i32 { SceneList::Result as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> { Some(&self.count_down_scene_judgment) }

    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::GameEnd => true,
            _ => false,
        }
    }

    // ResultScene の後に検出するシーンがないので、結果の検出だけ行う
    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        async_std::task::block_on(async {
            self.count_down_scene_judgment.match_captured_scene(capture_image).await;
        });
        if !self.count_down_scene_judgment.is_near_match() {
            return Ok(false);
        }

        let smashbros_data = match smashbros_data {
            Some(smashbros_data) => smashbros_data,
            None => return Ok(false),
        };

        match smashbros_data.get_player_count() {
            2 => self.result_with_2(capture_image, smashbros_data),
            4 => self.result_with_4(capture_image, smashbros_data),
            _ => Ok(false) // TODO?: 8 人対戦とか?, 3人もあるらしい…
        }
    }

    // 結果画面からは ReadyToFight の検出もあるけど、Dialog によって連戦が予想されるので Result へ
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::Result }

    fn recoding_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<()> {
        async_std::task::block_on(async {
            self.retry_battle_scene_judgment.match_captured_scene(capture_image).await;
        });
        if self.retry_battle_scene_judgment.is_near_match() {
            // 「同じ相手との再戦を希望しますか？」のダイアログに一致してしまうと誤検出するので、そのフレームだけダミーの Mat を渡す
            self.buffer.recoding_frame(&core::Mat::default())
        } else {
            self.buffer.recoding_frame(capture_image)
        }
    }
    fn is_recoded(&self) -> bool { self.buffer.is_filled() }
    fn detect_data(&mut self, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let result_power_mask = &self.result_power_mask;
        let scene_judgment_list = &mut self.scene_judgment_list;
        self.buffer.replay_frame(|frame| {
            Self::captured_order(&frame, smashbros_data, scene_judgment_list)?;
            Self::captured_power(&frame, smashbros_data, result_power_mask)?;

            Ok(false)
        })?;
        Ok(())
    }
}
impl ResultScene {
    const ORDER_AREA_POS: [[core::Point; 4]; 2] = [
        [core::Point{x:205, y:4}, core::Point{x:470, y:4}, core::Point{x:0, y:0}, core::Point{x:0, y:0}],
        [core::Point{x: 90, y:0}, core::Point{x:250, y:0}, core::Point{x:420, y:0}, core::Point{x:580, y:0}]
    ];

    // 1 on 1
    fn result_with_2(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        self.result_scene_judgment(capture_image, smashbros_data)?;
        Ok(false)
    }
    // smash
    fn result_with_4(&mut self, _capture_image: &core::Mat, _smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        Ok(false)
    }

    // 結果画面を検出, retry_battle_scene_judgment の精度がよくなったので、検出出来る時にくる
    fn result_scene_judgment(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        if smashbros_data.all_decided_result() {
            // すべてのプレイヤーが確定している場合は判定すら行わない (matchTemaplte は処理コストが高い)
            return Ok(false);
        }
        if self.buffer.is_filled() {
            if !self.buffer.is_replay_end() {
                return Ok(false);
            }
        }

        if !self.buffer.is_recoding_started() {
            self.buffer.start_recoding_by_time(std::time::Duration::from_secs(3));
            self.buffer.recoding_frame(capture_image)?;
        }

        if self.scene_judgment_list.iter().any( |scene_judgment| scene_judgment.is_near_match() ) {
            // 順位の判定はそのフレームがほしいので、0フレーム目から録画をする
            self.buffer.start_recoding_by_time(std::time::Duration::from_secs(3));
            self.buffer.recoding_frame(capture_image)?;
        }

        Ok(false)
    }

    /// 順位が検出されているフレームの処理
    pub fn captured_order(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, scene_judgment_list: &mut Vec<SceneJudgment>) -> opencv::Result<bool> {
        let index_by_player_max = smashbros_data.get_player_count()/2-1;
        for player_number in 0..smashbros_data.get_player_count() {
            let order_number_pos = &Self::ORDER_AREA_POS[index_by_player_max as usize][player_number as usize];
            let order_number_area_image = core::Mat::roi(&capture_image.clone(),
                core::Rect{x:order_number_pos.x, y:order_number_pos.y, width:80, height:80})?;

            for order_count in 0..smashbros_data.get_player_count() {
                let scene_judgment = &mut scene_judgment_list[order_count as usize];
                async_std::task::block_on(async {
                    scene_judgment.match_captured_scene(&order_number_area_image).await;

                    if scene_judgment.is_near_match() {
                        smashbros_data.guess_order(player_number, order_count+1);
                    }
                });
            }
        }

        Ok(smashbros_data.all_decided_order())
    }

    // 戦闘力が検出されているフレームの処理
    pub fn captured_power(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, result_power_mask: &core::Mat) -> opencv::Result<bool> {
        use regex::Regex;
        // 戦闘力の位置を切り取って
        let mut temp_capture_image = core::Mat::default();
        let mut gray_number_area_image = core::Mat::default();
        utils::cvt_color_to(&capture_image, &mut gray_number_area_image, ColorFormat::GRAY as i32)?;
        core::bitwise_and(&gray_number_area_image, result_power_mask, &mut temp_capture_image, &core::no_array())?;

        // プレイヤー毎に処理する
        let (width, height) = (capture_image.cols(), capture_image.rows());
        let player_area_width = width / smashbros_data.get_player_count();
        let re = Regex::new(r"[^\d]+").unwrap();
        for player_number in 0..smashbros_data.get_player_count() {
            // 適当に小さくする
            let player_power_area = core::Rect {
                x: player_area_width*player_number, y: height/4, width: player_area_width, height: height/2
            };
            let mut power_area_image = core::Mat::roi(&temp_capture_image, player_power_area)?;
            let gray_power_area_image = core::Mat::roi(&gray_number_area_image, player_power_area)?;

            // 輪郭捕捉して(maskで切り取った戦闘力の領域)
            let mut power_contour_image = utils::trimming_any_rect(
                &mut power_area_image, &gray_power_area_image, None, None, None, false, None)?;

            // 近似白黒処理して
            let mut work_capture_image = core::Mat::default();
            imgproc::threshold(&power_contour_image, &mut work_capture_image, 200.0, 255.0, imgproc::THRESH_BINARY)?;

            // 輪郭捕捉して(数値の範囲)
            let power_contour_image = utils::trimming_any_rect(
                &mut power_contour_image, &work_capture_image, Some(1), Some(1.0), None, false, None)?;
            utils::cvt_color_to(&power_contour_image, &mut power_area_image, ColorFormat::RGB as i32)?;

            // tesseract で文字(数値)を取得して, 余計な文字を排除
            let text = &async_std::task::block_on(utils::run_ocr_with_number(&power_area_image, Some("0123456789"), false)).unwrap().to_string();
            let number = re.split(text).collect::<Vec<&str>>().join("");
            smashbros_data.guess_power( player_number, number.parse().unwrap_or(-1) );
        }

        Ok(smashbros_data.all_decided_power())
    }
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

            // 初期化されていなかったら sub を再び main にする
            if scene_manager.smashbros_data.is_finished_battle() {
                scene_manager.smashbros_data = scene_manager.sub_smashbros_data.clone();
            }
            scene_manager.sub_smashbros_data = SmashbrosData::default();

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
        own.registory_scene_event(SceneList::Unknown, SceneList::EndResultReplay, Box::new(|smashbros_data: &mut SmashbrosData| {
            if smashbros_data.get_id().is_some() {
                smashbros_data.update_battle();
            } else {
                smashbros_data.save_battle();
            }
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
    pub fn update_scene_list(&mut self) -> opencv::Result<()> {
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

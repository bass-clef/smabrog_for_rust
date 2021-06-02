
use opencv::{
    core,
    imgcodecs,
    imgproc,
    prelude::*
};

use crate::capture::*;
use crate::data::*;
use crate::gui::{
    GUI,
    Message,
};
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
            judgment_type: ColorFormat::RGB,
            border_match_ratio: 0.98,
            prev_match_ratio: 0f64,
            prev_match_point: Default::default(),
        }
    }
}
impl SceneJudgment {
    /// color_format に .*image を強制して、一致させるシーン
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

    /// change match border. default is 0.99
    fn set_border(mut self, border_match_ratio: f64) -> Self {
        self.border_match_ratio = border_match_ratio;

        self
    }
    
    /// キャプチャされた画像とシーンとをテンプレートマッチングして、一致した確率と位置を返す
    async fn match_captured_scene(&mut self, captured_image: &core::Mat) {
        let mut result = core::Mat::default();
        let mut converted_captured_image = core::Mat::default();
        utils::cvt_color_to(captured_image, &mut converted_captured_image, self.judgment_type as i32).unwrap();

        match self.judgment_type {
            ColorFormat::NONE => (),
            ColorFormat::RGB | ColorFormat::GRAY => {
                // [2値 | RGB]画像はマスクがあれば and かけて、ないならテンプレートマッチング
                // None の場合は converted_captured_image はコピーされた状態だけでよい
                if let Some(mask_image) = &self.mask_image {
                    // captured_image を mask_image で篩いにかけて,無駄な部分を削ぐ
                    // どうでもいいけどソースをみてそれに上書きしてほしいとき、同じ変数を指定できないの欠陥すぎね？？？(これが安全なメモリ管理か、、、。)
                    let mut temp_captured_image = converted_captured_image.clone();
                    core::bitwise_and(&converted_captured_image, &mask_image,
                        &mut temp_captured_image, &core::no_array().unwrap()).unwrap();
                        converted_captured_image = temp_captured_image;
                }

                imgproc::match_template(&converted_captured_image, &self.color_image, &mut result,
                    imgproc::TM_CCOEFF_NORMED, &core::no_array().unwrap()).unwrap();
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
            &mut 0.0, &mut self.prev_match_ratio,
            &mut core::Point::default(), &mut self.prev_match_point,
            &core::no_array().unwrap()
        ).unwrap();
    }

    /// 前回のテンプレートマッチングで大体一致しているか
    pub fn is_near_match(&self) -> bool {
        self.border_match_ratio <= self.prev_match_ratio
    }
}

/// シーン雛形 (動作は子による)
pub trait SceneTrait {
    /// シーン識別ID
    fn get_id(&self) -> i32;
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

    fn draw(&self, capture: &mut core::Mat);
}

use strum::IntoEnumIterator;
use strum_macros::EnumIter;
#[derive(Clone, Copy, Debug, EnumIter, PartialEq)]
pub enum SceneList {
    Unknown = 0, Loading, Dialog, ReadyToFight, Matching,
    HamVsSpam, GameStart, GamePlaying, GameEnd, Result,
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

/// 状態不明のシーン
#[derive(Default)]
struct UnknownScene {}
impl SceneTrait for UnknownScene {
    fn get_id(&self) -> i32 { SceneList::Unknown as i32 }

    // 状態不明は他から遷移する、もしくは最初のシーンなので, 自身ではならない, 他に移らない, 録画しない,, データ検出しない
    fn continue_match(&self, _now_scene: SceneList) -> bool { false }
    fn is_scene(&mut self, _capture_image: &core::Mat, _smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> { Ok(false) }
    fn to_scene(&self, now_scene: SceneList) -> SceneList { now_scene }
    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }

    fn draw(&self, _capture: &mut core::Mat) {}
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
        }
    }
}
impl SceneTrait for LoadingScene {
    fn get_id(&self) -> i32 { SceneList::Loading as i32 }

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

    fn draw(&self, _capture: &mut core::Mat) {}
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

    fn draw(&self, _capture: &mut core::Mat) {}
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

    // 回線切断などの原因で最初に戻ることは常にあるので gray match だし常に判定だけしておく
    fn continue_match(&self, now_scene: SceneList) -> bool {
        if now_scene == SceneList::Unknown {
            // 未検出だと ReadyToFight の検出率を表示
            let prev_ratio = vec![
                self.red_scene_judgment.prev_match_ratio, self.grad_scene_judgment.prev_match_ratio
            ].iter().fold(0.0/0.0, |m, v| v.max(m));
            GUI::set_title(&format!("not {}", prev_ratio ));
        }

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
    fn draw(&self, _capture: &mut core::Mat) {}
}
impl ReadyToFightScene {
    pub fn new_gray() -> Self {
        Self {
            grad_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/ready_to_fight_color_0.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/ready_to_fight_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            red_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/ready_to_fight_color_1.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/ready_to_fight_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
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
}
impl Default for MatchingScene {
    fn default() -> Self {
        Self {
            scene_judgment: SceneJudgment::new(
                    imgcodecs::imread("resource/ready_ok_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/ready_ok_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.95),
            scene_judgment_with4: SceneJudgment::new(
                    imgcodecs::imread("resource/with_4_battle_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/with_4_battle_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap(),
        }
    }
}
impl SceneTrait for MatchingScene {
    fn get_id(&self) -> i32 { SceneList::Matching as i32 }
    
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::Unknown | SceneList::ReadyToFight | SceneList::Result => true,
            _ => false,
        }
    }

    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        // 多分 1on1 のほうが多いかな？
        async_std::task::block_on(async {
            self.scene_judgment.match_captured_scene(&capture_image).await;
            if self.scene_judgment.is_near_match() {
                return; // async-function
            }

            self.scene_judgment_with4.match_captured_scene(&capture_image).await;
        });

        if self.scene_judgment.is_near_match() || self.scene_judgment_with4.is_near_match() {
            if let Some(smashbros_data) = smashbros_data {
                if self.scene_judgment.is_near_match() {
                    smashbros_data.initialize_battle(2);
                } else if self.scene_judgment_with4.is_near_match() {
                    smashbros_data.initialize_battle(4);
                }
            }
        }

        Ok( self.scene_judgment.is_near_match() || self.scene_judgment_with4.is_near_match() )
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::Matching }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }

    fn draw(&self, _capture: &mut core::Mat) {}
}

/// キャラクターが大きく表示されてる画面
/// save: キャラクター名, ルール名, 取れるなら[時間,ストック,HP]
struct HamVsSpamScene {
    vs_scene_judgment: SceneJudgment,
    rule_time_scene_judgment: SceneJudgment,
    buffer: CaptureFrameStore,
}
impl Default for HamVsSpamScene {
    fn default() -> Self {
        Self {
            vs_scene_judgment: SceneJudgment::new(
                imgcodecs::imread("resource/vs_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/vs_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            rule_time_scene_judgment: SceneJudgment::new(
                imgcodecs::imread("resource/rule_time_stock_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/rule_time_stock_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            buffer: CaptureFrameStore::default(),
        }
    }
}
impl SceneTrait for HamVsSpamScene {
    fn get_id(&self) -> i32 { SceneList::HamVsSpam as i32 }
    
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::Matching => true,
            _ => false,
        }
    }

    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        if let Some(smashbros_data) = smashbros_data {
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
            self.buffer.start_recoding_by_time(std::time::Duration::from_secs(3));
            self.buffer.recoding_frame(capture_image)?;
        }
        Ok(self.vs_scene_judgment.is_near_match())
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::HamVsSpam }

    fn recoding_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<()> { self.buffer.recoding_frame(capture_image) }
    fn is_recoded(&self) -> bool { self.buffer.is_filled() }

    /// save: キャラクターの種類, ルール(time | stock), 時間
    fn detect_data(&mut self, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let rule_time_scene_judgment = &mut self.rule_time_scene_judgment;
        self.buffer.replay_frame(|frame| {
            let ref_frame = &frame;
            Ok(
                Self::captured_rules(ref_frame, smashbros_data, rule_time_scene_judgment)?
                & Self::captured_character_name(ref_frame, smashbros_data)?
            )
        })?;

        Ok(())
    }

    fn draw(&self, _capture: &mut core::Mat) {}
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
        imgproc::threshold(&gray_capture_image, &mut work_capture_image, 250.0, 255.0, imgproc::THRESH_BINARY)?;
        core::bitwise_not(&work_capture_image, &mut temp_capture_image, &core::no_array()?)?;

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
                width: player_area_width -10 -30, height: height/7  // 10:稲妻が処理後に黒四角形になって文字領域として誤認されるのを防ぐため
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

    pub fn captured_rules(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, rule_scene_judgment: &mut SceneJudgment) -> opencv::Result<bool> {
        if smashbros_data.is_decided_rule() && smashbros_data.all_decided_max_stock() {
            return Ok(true)
        }

        if !smashbros_data.is_decided_rule() {
            // タイム制と検出(1on1: これがデフォルトで一番多いルール)
            async_std::task::block_on(async {
                rule_scene_judgment.match_captured_scene(capture_image).await;
            });
            if rule_scene_judgment.is_near_match() {
                smashbros_data.set_rule(BattleRule::Stock)
            }
        }

        // 各ルール条件の検出
        let mut time_area: Option<core::Mat> = None;
        let mut stock_area: Option<core::Mat> = None;
        let mut hp_area: Option<core::Mat> = None;
        match smashbros_data.get_rule() {
            BattleRule::Time => {
            },
            BattleRule::Stock => {
                // 制限時間の位置を切り取って
                // time: xy:275x335, wh:9x12
                time_area = Some(core::Mat::roi( capture_image, core::Rect {x:275, y:335, width:9, height:12}).unwrap() );
                // stock: xy:359x335, wh:9x12
                stock_area = Some(core::Mat::roi( capture_image, core::Rect {x:359, y:335, width:9, height:12}).unwrap() );
            },
            BattleRule::Stamina => {

            },
            _ => ()
        }
        if let Some(mut time_area) = time_area {
            Self::captured_time(&mut time_area, smashbros_data)?;
        }
        if let Some(mut stock_area) = stock_area {
            Self::captured_stock(&mut stock_area, smashbros_data)?;
        }
        if let Some(mut hp_area) = hp_area {
            Self::captured_stamina(&mut hp_area, smashbros_data)?;
        }

        Ok(smashbros_data.is_decided_rule() && smashbros_data.is_decided_max_time() && smashbros_data.all_decided_max_stock())
    }

    pub fn captured_time(capture_image: &mut core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let time = std::time::Duration::from_secs(
            match Self::captured_convert_string(capture_image, r"\s*(\d)\s*") {
                Ok(time) => time.parse::<u64>().unwrap_or(0) * 60,
                Err(_) => 0,
            }
        );
        println!("time {:?}", time);
        smashbros_data.set_max_time(time);

        Ok(())
    }

    pub fn captured_stock(capture_image: &mut core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let stock: i32 = match Self::captured_convert_string(capture_image, r"\s*(\d)\s*") {
            Ok(stock) => stock.parse().unwrap_or(-1),
            Err(_) => 0,
        };

        for player_number in 0..smashbros_data.get_player_count() {
            smashbros_data.guess_max_stock(player_number, stock);
        }

        Ok(())
    }

    pub fn captured_stamina(capture_image: &mut core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {

        Ok(())
    }

    /// capture_image から検出した文字列を regex_pattern で正規表現にかけて返す
    pub fn captured_convert_string(capture_image: &mut core::Mat, regex_pattern: &str) -> opencv::Result<String> {
        use regex::Regex;
        // 近似白黒処理して
        let mut gray_capture_image = core::Mat::default();
        imgproc::threshold(capture_image, &mut gray_capture_image, 250.0, 255.0, imgproc::THRESH_BINARY)?;
        let mut work_capture_image = core::Mat::default();
        utils::cvt_color_to(&gray_capture_image, &mut work_capture_image, ColorFormat::GRAY as i32)?;

        // 輪郭捕捉して(数値の範囲)
        let contour_image = utils::trimming_any_rect(
            capture_image, &work_capture_image, Some(1), Some(1.0), None, false, None)?;
        let mut gray_contour_image = core::Mat::default();
        utils::cvt_color_to(&contour_image, &mut gray_contour_image, ColorFormat::RGB as i32)?;

        // tesseract で文字(数値)を取得して, 余計な文字を排除
        let text = &async_std::task::block_on(utils::run_ocr_with_number(&gray_contour_image)).unwrap().to_string();
        let re = Regex::new(regex_pattern).unwrap();
        if let Some(caps) = re.captures( text ) {
            return Ok( caps[1].to_string() );
        }

        Err(opencv::Error::new( 0, "not found anything. from captured_convert_string".to_string() ))
    }
}

/// 試合開始の検出
struct GameStartScene {
    scene_judgment: SceneJudgment,
}
impl Default for GameStartScene {
    fn default() -> Self {
        Self {
            scene_judgment: SceneJudgment::new_trans(
                imgcodecs::imread("resource/battle_time_zero_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/battle_time_zero_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
        }
    }
}
impl SceneTrait for GameStartScene {
    fn get_id(&self) -> i32 { SceneList::GameStart as i32 }
    
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
        async_std::task::block_on(async {
            self.scene_judgment.match_captured_scene(&capture_image).await;
        });

        if 0.85 < self.scene_judgment.prev_match_ratio &&
            568 == self.scene_judgment.prev_match_point.x && 13 == self.scene_judgment.prev_match_point.y {
                if let Some(smashbros_data) = smashbros_data {
                    smashbros_data.start_battle();
                }
                return Ok(true);
        }

        Ok(false)
    }

    // now_scene が GameStart になることはない("GO"を検出した時はもう GamePlaying であるため)
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GamePlaying }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
    
    fn draw(&self, _capture: &mut core::Mat) {}
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
            ).unwrap(),
            stock_white_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/stock_hyphen_color_white.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/stock_hyphen_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            buffer: CaptureFrameStore::default(),
            stock_number_mask: imgcodecs::imread("resource/stock_number_mask.png", imgcodecs::IMREAD_GRAYSCALE).unwrap()
        }
    }
}
impl SceneTrait for GamePlayingScene {
    fn get_id(&self) -> i32 { SceneList::GamePlaying as i32 }
    
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

    fn draw(&self, _capture: &mut core::Mat) {}
}
impl GamePlayingScene {
    // 1 on 1
    fn game_playing_with_2(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        self.stock_scene_judgment(capture_image, smashbros_data)?;
        Ok(false)
    }
    // smash
    fn game_playing_with_4(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        Ok(false)
    }

    // ストックを検出
    fn stock_scene_judgment(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        if smashbros_data.all_decided_stock() {
            // すべてのプレイヤーが確定している場合は判定すら行わない (matchTemaplte は処理コストが高い)
            return Ok(false);
        }

        async_std::task::block_on(async {
            self.stock_black_scene_judgment.match_captured_scene(&capture_image).await;
            if self.stock_black_scene_judgment.is_near_match() {
                return; // async-function
            }
            
            self.stock_white_scene_judgment.match_captured_scene(&capture_image).await;
        });

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
        core::bitwise_and(&gray_number_area_image, stock_number_mask, &mut temp_capture_image, &core::no_array()?)?;

        // 近似白黒処理して
        let mut work_capture_image = core::Mat::default();
        imgproc::threshold(&temp_capture_image, &mut work_capture_image, 250.0, 255.0, imgproc::THRESH_BINARY)?;
        core::bitwise_and(&gray_number_area_image, &work_capture_image, &mut temp_capture_image, &core::no_array()?)?;
        core::bitwise_not(&temp_capture_image, &mut work_capture_image, &core::no_array()?)?;

        // プレイヤー毎に処理する
        let (width, height) = (capture_image.cols(), capture_image.rows());
        let player_area_width = width / smashbros_data.get_player_count();
        let re = Regex::new(r"\s*(\w+)\s*").unwrap();
        let mut skip_count = 0;
        for player_count in 0..smashbros_data.get_player_count() {
            if smashbros_data.is_decided_stock(player_count) {
                // 既にプレイヤーのストックが確定しているならスキップ
                skip_count += 1;
                continue;
            }
            // 適当に小さくする
            let player_stock_area = core::Rect {
                x: player_area_width*player_count, y: height/4, width: player_area_width, height: height/2
            };
            let mut stock_area_image = core::Mat::roi(&work_capture_image, player_stock_area)?;
            let gray_stock_area_image = core::Mat::roi(&gray_number_area_image, player_stock_area)?;

            // 輪郭捕捉して
            let stock_contour_image = utils::trimming_any_rect(
                &mut stock_area_image, &gray_stock_area_image, Some(5), Some(1000.0), None, true, None)?;

            // tesseract で文字(数値)を取得して, 余計な文字を排除
            let text = &async_std::task::block_on(utils::run_ocr_with_number(&stock_contour_image)).unwrap().trim().to_string();
            if let Some(caps) = re.captures( text ) {
                smashbros_data.guess_stock( player_count, (&caps[1]).parse().unwrap_or(-1) );
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
            game_set_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/game_set_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/game_set_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_border(0.95),
            time_up_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/time_up_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/time_up_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_border(0.95),
        }
    }
}
impl SceneTrait for GameEndScene {
    fn get_id(&self) -> i32 { SceneList::GameEnd as i32 }
    
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::GamePlaying => true,
            _ => false,
        }
    }

    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        // おそらく、ストック制を選択してる人のほうが多い(もしくは時間より先に決着がつくことが多い)
        async_std::task::block_on(async {
            self.game_set_scene_judgment.match_captured_scene(&capture_image).await;
            if self.game_set_scene_judgment.is_near_match() {
                return; // async-function
            }

            self.time_up_scene_judgment.match_captured_scene(&capture_image).await;
        });

        if self.game_set_scene_judgment.is_near_match() || self.time_up_scene_judgment.is_near_match() {
            if let Some(smashbros_data) = smashbros_data {
                smashbros_data.finish_battle();
            }
        }

        Ok(self.game_set_scene_judgment.is_near_match() || self.time_up_scene_judgment.is_near_match())
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GameEnd }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }

    fn draw(&self, _capture: &mut core::Mat) {}
}

/// 結果画面表示
/// save: プレイヤー毎の[戦闘力, 順位]
struct ResultScene {
    scene_judgment_list: Vec<SceneJudgment>,
    buffer: CaptureFrameStore,
    result_power_mask: core::Mat
}
impl Default for ResultScene {
    fn default() -> Self {
        let mut scene_judgment_list = vec![];
        for player_number in 1..5 {
            let path = format!("resource/result_player_order_{}_", player_number);
            scene_judgment_list.push(
                SceneJudgment::new_trans(
                    imgcodecs::imread(&(path.clone() + "color.png"), imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread(&(path + "mask.png"), imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.99)
            );
        }

        Self {
            scene_judgment_list: scene_judgment_list,
            buffer: CaptureFrameStore::default(),
            result_power_mask: imgcodecs::imread("resource/result_power_mask.png", imgcodecs::IMREAD_GRAYSCALE).unwrap(),
        }
    }
}
impl SceneTrait for ResultScene {
    fn get_id(&self) -> i32 { SceneList::Unknown as i32 }

    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::GameEnd => true,
            _ => false,
        }
    }

    // ResultScene の後に検出するシーンがないので、結果の検出だけ行う
    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        let smashbros_data = match smashbros_data {
            Some(smashbros_data) => smashbros_data,
            None => return Ok(false),
        };

        match smashbros_data.get_player_count() {
            2 => self.result_with_2(capture_image, smashbros_data),
            4 => self.result_with_4(capture_image, smashbros_data),
            _ => Ok(false) // TODO?: 8 人対戦とか?
        }
    }

    // 結果画面からは ReadyToFight の検出もあるけど、Dialog によって連戦が予想されるので Result へ
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::Result }

    fn recoding_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<()> { self.buffer.recoding_frame(capture_image) }
    fn is_recoded(&self) -> bool { self.buffer.is_filled() }
    fn detect_data(&mut self, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let result_power_mask = &self.result_power_mask;
        let scene_judgment_list = &mut self.scene_judgment_list;
        self.buffer.replay_frame(|frame| {
            Ok(
                Self::captured_order(&frame, smashbros_data, scene_judgment_list)?
                & Self::captured_power(&frame, smashbros_data, result_power_mask)?
            )
        })?;
        Ok(())
    }

    fn draw(&self, _capture: &mut core::Mat) {}
}
impl ResultScene {
    const ORDER_POS: [[core::Point; 4]; 2] = [
        [core::Point{x:200, y:0}, core::Point{x:470, y:0}, core::Point{x:0, y:0}, core::Point{x:0, y:0}],
        [core::Point{x: 90, y:0}, core::Point{x:250, y:0}, core::Point{x:420, y:0}, core::Point{x:580, y:0}]
    ];

    // 1 on 1
    fn result_with_2(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        self.result_scene_judgment(capture_image, smashbros_data)?;
        Ok(false)
    }
    // smash
    fn result_with_4(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        Ok(false)
    }

    // 結果画面を検出
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

        // とりあえず、どちらかの結果画面が検出されたら録画を始める (順位の数字によって検出)
        let index_by_player_max = smashbros_data.get_player_count()/2-1;
        for player_number in 0..smashbros_data.get_player_count() {
            let order_number_pos = &Self::ORDER_POS[index_by_player_max as usize][player_number as usize];
            let order_number_area_image = core::Mat::roi(&capture_image.clone(),
                core::Rect{x:order_number_pos.x, y:order_number_pos.y, width:100, height:100})?;

            for order_count in 0..smashbros_data.get_player_count() {
                async_std::task::block_on(async {
                    self.scene_judgment_list[order_count as usize].match_captured_scene(&order_number_area_image).await;
                });
            }
        }

        if self.scene_judgment_list.iter().any( |scene_judgment| scene_judgment.is_near_match() ) {
            self.buffer.start_recoding_by_time(std::time::Duration::from_secs(5));
            self.buffer.recoding_frame(capture_image)?;
        }

        Ok(false)
    }

    /// 順位が検出されているフレームの処理
    pub fn captured_order(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, scene_judgment_list: &mut Vec<SceneJudgment>) -> opencv::Result<bool> {
        let index_by_player_max = smashbros_data.get_player_count()/2-1;
        for player_number in 0..smashbros_data.get_player_count() {
            if smashbros_data.is_decided_order(player_number) {
                continue;
            }

            let order_number_pos = &Self::ORDER_POS[index_by_player_max as usize][player_number as usize];
            let order_number_area_image = core::Mat::roi(&capture_image.clone(),
                core::Rect{x:order_number_pos.x, y:order_number_pos.y, width:100, height:100})?;

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

        Ok(false)
    }

    // 戦闘力が検出されているフレームの処理
    pub fn captured_power(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, result_power_mask: &core::Mat) -> opencv::Result<bool> {
        use regex::Regex;
        // 戦闘力の位置を切り取って
        let mut temp_capture_image = core::Mat::default();
        let mut gray_number_area_image = core::Mat::default();
        utils::cvt_color_to(&capture_image, &mut gray_number_area_image, ColorFormat::GRAY as i32)?;
        core::bitwise_and(&gray_number_area_image, result_power_mask, &mut temp_capture_image, &core::no_array()?)?;

        // プレイヤー毎に処理する
        let (width, height) = (capture_image.cols(), capture_image.rows());
        let player_area_width = width / smashbros_data.get_player_count();
        let re = Regex::new(r"[^\d]+").unwrap();
        let mut skip_count = 0;
        for player_count in 0..smashbros_data.get_player_count() {
            if smashbros_data.is_decided_power(player_count) {
                // 既にプレイヤーのストックが確定しているならスキップ
                skip_count += 1;
                continue;
            }
            // 適当に小さくする
            let player_power_area = core::Rect {
                x: player_area_width*player_count, y: height/4, width: player_area_width, height: height/2
            };
            let mut power_area_image = core::Mat::roi(&temp_capture_image, player_power_area)?;
            let gray_power_area_image = core::Mat::roi(&gray_number_area_image, player_power_area)?;

            // 輪郭捕捉して(maskで切り取った戦闘力の領域)
            let mut power_contour_image = utils::trimming_any_rect(
                &mut power_area_image, &gray_power_area_image, None, None, None, false, None)?;

            // 近似白黒処理して
            let mut work_capture_image = core::Mat::default();
            imgproc::threshold(&power_contour_image, &mut work_capture_image, 127.0, 255.0, imgproc::THRESH_BINARY)?;

            // 輪郭捕捉して(数値の範囲)
            let power_contour_image = utils::trimming_any_rect(
                &mut power_contour_image, &work_capture_image, Some(1), Some(1.0), None, false, None)?;
            utils::cvt_color_to(&power_contour_image, &mut power_area_image, ColorFormat::RGB as i32)?;

            // tesseract で文字(数値)を取得して, 余計な文字を排除
            let text = &async_std::task::block_on(utils::run_ocr_with_number(&power_area_image)).unwrap().to_string();
            let number = re.split(text).collect::<Vec<&str>>().join(""); // 5桁まで (\d,\d,\d,\d,\d)
            smashbros_data.guess_power( player_count, number.parse().unwrap_or(-1) );
        }

        Ok(smashbros_data.get_player_count() == skip_count)
    }
}


/// シーン全体を非同期で管理するクラス
pub struct SceneManager {
    pub capture: Box<dyn CaptureTrait>,
    pub scene_loading: LoadingScene,
    pub scene_list: Vec<Box<dyn SceneTrait + 'static>>, // koko: 'staticいらないのでは？
    pub now_scene: SceneList,
    pub smashbros_data: SmashbrosData,
    pub dummy_local_time: chrono::DateTime<chrono::Local>,
}
impl Default for SceneManager {
    fn default() -> Self {
        Self {
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
            dummy_local_time: chrono::Local::now(),
        }
    }
}
impl SceneManager {
    pub fn get_now_data(&self) -> SmashbrosData {
        let mut cloned_data = self.smashbros_data.clone();
        // 重複操作されないために適当な時間で保存済みのデータにする
        cloned_data.set_id(Some("dummy_id".to_string()));
        
        cloned_data
    }

    pub fn update(&mut self) -> opencv::Result<Option<Message>> {
        let mut capture_image = self.capture.get_mat()?;

        if self.scene_loading.is_scene(&capture_image, None)? {
            // 読込中の画面(真っ黒に近い)はテンプレートマッチングで 1.0 がでてしまうので回避
            return Ok(None);
        }

        // 現在キャプチャと比較して遷移する
        for scene in &mut self.scene_list[..] {
            scene.draw(&mut capture_image);

            // シーンによって適切な時に録画される
            scene.recoding_scene(&capture_image)?;
            if scene.is_recoded() {
                // 所謂ビデオ判定
                scene.detect_data(&mut self.smashbros_data)?
            }

            // よけいな match をさけるため(is_scene すること自体が結構コストが高い)
            if !scene.continue_match(self.now_scene) {
                continue;
            }

            // 遷移?
            if scene.is_scene(&capture_image, Some(&mut self.smashbros_data))? {
                println!(
                    "[{:?}] match {:?} to {:?}",
                    SceneList::to_scene_list(scene.get_id()),
                    self.now_scene, scene.to_scene(self.now_scene)
                );
                
                self.now_scene = scene.to_scene(self.now_scene);
            }
        }

        opencv::highgui::imshow("capture_image", &capture_image)?;

        Ok(None)
    }
}

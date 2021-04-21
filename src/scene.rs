
use opencv::{
    core,
    imgcodecs,
    imgproc,
    prelude::*
};
use tesseract::Tesseract;


use crate::capture::*;
use crate::gui::GUI;

macro_rules! measure {
    ( $x:expr) => {{
        let start = std::time::Instant::now();
        let result = $x;
        let end = start.elapsed();

        println!("{}.{:03}s", end.as_secs(), end.subsec_nanos() / 1_000_000);
    }};
}

#[derive(Copy, Clone)]
pub enum ColorFormat {
    NONE = 0, GRAY = 1,
    RGB = 3, RGBA = 4,
}

pub struct Utils {}
impl Utils {
    // if-else も match もさけようね～というやつ
    const COLOR_MAP: [[i32; 5]; 5] = [
        [-1, -1, -1, -1, -1],    // null to null
        [-1, -1, -1, imgproc::COLOR_GRAY2RGB, imgproc::COLOR_GRAY2RGBA],   // gray to hoge
        [-1, -1, -1, -1, -1],    // unknown to unknown
        [-1, imgproc::COLOR_RGB2GRAY, -1, -1, imgproc::COLOR_RGB2RGBA],   // rgb to hoge
        [-1, imgproc::COLOR_RGBA2GRAY, -1, imgproc::COLOR_RGBA2RGB, 0],   // rgba to hoge
    ];

    /// src.channels() に応じて to_channels に変換する cvt_color をする
    /// src.channels() == to_channels なら src.copy_to(dst) をする
    pub fn cvt_color_to(src: &core::Mat, dst: &mut core::Mat, to_channels: i32) -> opencv::Result<()> {
        let color_map = Utils::COLOR_MAP[src.channels()? as usize][to_channels as usize];
        if -1 == color_map {
            // コピーだけする
            src.copy_to(dst)?;
            return Ok(());
        }

        imgproc::cvt_color(src, dst, color_map, 0)?;
        Ok(())
    }

    /// OpenCV に処理するメソッドがないため定義。(NaN はあるのにどうして inf は無いんだ？？？)
    pub fn patch_inf_ns(data: &mut core::Mat, to_value: f32) -> opencv::Result<()> {
        for y in 0..data.cols() {
            for x in 0..data.rows() {
                let value = data.at_mut::<f32>(y * data.rows() + x)?;
                if *value == std::f32::INFINITY || *value == std::f32::NEG_INFINITY {
                    *value = to_value;
                }
            }
        }
        Ok(())
    }

    /// src に対しての特定色を透過色とした mask を作成
    pub fn make_trans_mask_from_noalpha(src: &core::Mat, dst: &mut core::Mat) -> opencv::Result<()> {
        let trans_color = [0.0, 0.0, 0.0, 1.0];
        let lower_mat = core::Mat::from_slice(&trans_color)?;
        let upper_mat = core::Mat::from_slice(&trans_color)?;
        let mut mask = core::Mat::default();
        core::in_range(&src, &lower_mat, &upper_mat, &mut mask)?;
        core::bitwise_not(&mask, dst, &core::no_array()?)?;

        Ok(())
    }

    /// 任意の四角形の中にある何かの輪郭にそって src を加工して返す
    pub fn trimming_any_rect(src: &mut core::Mat, gray_src: &core::Mat, margin: Option<i32>,
        min_size: Option<f64>, max_size: Option<f64>, noise_fill: bool, noise_color: Option<core::Scalar>)
    -> opencv::Result<core::Mat>
    {
        let mut contours = opencv::types::VectorOfVectorOfPoint::new();
        let (width, height) = (src.cols(), src.rows());
        let mut any_rect = core::Rect::new(width, height, 0, 0);
        imgproc::find_contours(gray_src, &mut contours, imgproc::RETR_EXTERNAL, imgproc::CHAIN_APPROX_SIMPLE, core::Point{x:0,y:0})?;

        for (i, contour) in &mut contours.to_vec().iter_mut().enumerate() {
            let mut area_contours = opencv::types::VectorOfPoint::from_iter(contour.iter());
            let area = imgproc::contour_area(&area_contours, false)?;
            // ノイズの除去 or スキップ
            if area < min_size.unwrap_or(10.0) {
                if noise_fill {
                    imgproc::draw_contours(
                        src, &contours, i as i32, noise_color.unwrap_or(core::Scalar::new(255.0, 255.0, 255.0, 0.0)),
                        1, imgproc::LINE_8, &core::no_array()?, std::i32::MAX, core::Point{x:0,y:0})?;
                }
                continue;
            } else if max_size.unwrap_or(10_000.0) < area {
                if noise_fill && max_size.is_some() {
                    imgproc::draw_contours(
                        src, &contours, i as i32, noise_color.unwrap_or(core::Scalar::new(255.0, 255.0, 255.0, 0.0)),
                        1, imgproc::LINE_8, &core::no_array()?, std::i32::MAX, core::Point{x:0,y:0})?;
                }
                continue;
            }

            let rect = imgproc::bounding_rect(&area_contours)?;
            any_rect.x = std::cmp::min(any_rect.x, rect.x);
            any_rect.y = std::cmp::min(any_rect.y, rect.y);
            any_rect.width = std::cmp::max(any_rect.x, rect.x + rect.width);
            any_rect.height = std::cmp::max(any_rect.y, rect.y + rect.height);
        }

        let mut trimming_rect = core::Rect {
            x: std::cmp::max(any_rect.x - margin.unwrap_or(0), 0),
            y: std::cmp::max(any_rect.y - margin.unwrap_or(0), 0),
            width: std::cmp::min(any_rect.width + margin.unwrap_or(0), width),
            height: std::cmp::min(any_rect.height + margin.unwrap_or(0), height)};
        trimming_rect.width -= trimming_rect.x + 1;
        trimming_rect.height -= trimming_rect.y + 1;
        match core::Mat::roi(&src, trimming_rect) {
            Ok(result_image) => Ok(result_image),
            // size が 0 近似で作成できないときが予想されるので、src を返す
            Err(_) => Ok(src.clone()),
        }
    }

    /// ocr を Mat で叩く。
    /// tesseract::ocr_from_frame だと「Warning: Invalid resolution 0 dpi. Using 70 instead.」がうるさかったので作成
    pub async fn run_ocr(image: &core::Mat) -> Result<String, tesseract::TesseractError> {
        let size = image.channels().unwrap() * image.cols() * image.rows();
        let data: &[u8] = unsafe{ std::slice::from_raw_parts(image.datastart(), size as usize) };
        
        Ok(
            Tesseract::new(None, Some("eng"))?
                .set_frame(data, image.cols(), image.rows(),
                    image.channels().unwrap(), image.channels().unwrap() * image.cols())?
                .set_source_resolution(70)
                .recognize()?
                .get_text().unwrap_or("".to_string())
        )
    }
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
            border_match_ratio: 0.99,
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
        Utils::cvt_color_to(&color_image, &mut converted_color_image, color_format as i32)?;

        let converted_mask_image = match &mask_image {
            Some(v) => {
                let mut converted_mask_image = core::Mat::default();
                Utils::cvt_color_to(&v, &mut converted_mask_image, color_format as i32)?;

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
        Utils::cvt_color_to(&color_image, &mut converted_color_image, ColorFormat::RGBA as i32)?;

        // 透過画像の場合は予め透過マスクを作成する
        let mut converted_mask_image = core::Mat::default();
        let mut trans_mask_image = core::Mat::default();
        match &mask_image {
            Some(v) => {
                // 強制で RGBA にする
                Utils::cvt_color_to(&v, &mut trans_mask_image, ColorFormat::RGBA as i32)?;
                Utils::cvt_color_to(&v, &mut converted_mask_image, ColorFormat::RGBA as i32)?;
            },
            None => {
                Utils::make_trans_mask_from_noalpha(&color_image, &mut trans_mask_image)?;
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
        Utils::cvt_color_to(captured_image, &mut converted_captured_image, self.judgment_type as i32).unwrap();

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
        Utils::patch_inf_ns(&mut result, -0.0).unwrap();

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
    fn is_scene(&mut self, mat: &core::Mat) -> opencv::Result<bool>;
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
    fn is_scene(&mut self, _capture_image: &core::Mat) -> opencv::Result<bool> { Ok(false) }
    fn to_scene(&self, now_scene: SceneList) -> SceneList { now_scene }
    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }

    fn draw(&self, _capture: &mut core::Mat) {}
}

/// 読込中のシーン
struct LoadingScene {
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

    fn is_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<bool> {
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
    
    fn is_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<bool> {
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
}
impl Default for ReadyToFightScene {
    fn default() -> Self { Self::new_gray() }
}
impl SceneTrait for ReadyToFightScene {
    fn get_id(&self) -> i32 { SceneList::ReadyToFight as i32 }

    // 回線切断などの原因で最初に戻ることは常にあるので gray match だし常に判定だけしておく
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::ReadyToFight => false,
            _ => true,
        }
    }
    
    fn is_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<bool> {
        // 多分 red版 ReadyToFight のほうが多いので先にする
        async_std::task::block_on(async {
            self.red_scene_judgment.match_captured_scene(&capture_image).await;
            if self.red_scene_judgment.is_near_match() {
                return; // async-function
            }

            self.grad_scene_judgment.match_captured_scene(&capture_image).await;
        });

        Ok(self.red_scene_judgment.is_near_match() | self.grad_scene_judgment.is_near_match())
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
        }
    }
}

/// マッチング中の画面 (CPUと戦えるあの画面)
/// save: プレイヤー人数(2p, 4p)
struct MatchingScene {
    scene_judgment: SceneJudgment,
}
impl Default for MatchingScene {
    fn default() -> Self {
        Self {
            scene_judgment: SceneJudgment::new(
                imgcodecs::imread("resource/ready_ok_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/ready_ok_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_border(0.98)
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

    fn is_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<bool> {
        // ready_ok_color が下半分しかないｗ
        async_std::task::block_on(async {
            self.scene_judgment.match_captured_scene(&capture_image).await;
        });

        Ok(self.scene_judgment.is_near_match())
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::Matching }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { self.scene_judgment.is_near_match() }

    fn detect_data(&mut self, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        if self.scene_judgment.is_near_match() {
            smashbros_data.player_count = 2;
        }

        Ok(())
    }

    fn draw(&self, _capture: &mut core::Mat) {}
}

/// キャラクターが大きく表示されてる画面
struct HamVsSpamScene {
    scene_judgment: SceneJudgment,
    buffer: CaptureFrameStore,
}
impl Default for HamVsSpamScene {
    fn default() -> Self {
        Self {
            scene_judgment: SceneJudgment::new(
                imgcodecs::imread("resource/vs_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/vs_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
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

    fn is_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<bool> {
        async_std::task::block_on(async {
            self.scene_judgment.match_captured_scene(&capture_image).await;
        });

        if self.scene_judgment.is_near_match() {
            imgcodecs::imwrite("ham_vs_spam.png", capture_image, &core::Vector::from(vec![]))?;
            self.buffer.start_recoding_by_frame(5);
        }
        Ok(self.scene_judgment.is_near_match())
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::HamVsSpam }

    fn recoding_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<()> {
        self.buffer.recoding_frame(capture_image)?;
        Ok(())
    }
    fn is_recoded(&self) -> bool { self.buffer.is_filled() }

    /// save: キャラクターの種類, ルール(time | stock), 時間
    fn detect_data(&mut self, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        if self.buffer.is_recoding_end() {
            smashbros_data.character_name_list.clear();
            for player_count in 0..smashbros_data.player_count {
                smashbros_data.character_name_list.push("".to_string());
            }
        }

        use regex::Regex;
        self.buffer.replay_frame(|frame| {
            let mut gray_capture_image = core::Mat::default();
            Utils::cvt_color_to(&frame, &mut gray_capture_image, ColorFormat::GRAY as i32)?;
    
            // 近似白黒処理して
            let mut temp_capture_image = core::Mat::default();
            let mut work_capture_image = core::Mat::default();
            imgproc::threshold(&gray_capture_image, &mut work_capture_image, 250.0, 255.0, imgproc::THRESH_BINARY)?;
            core::bitwise_not(&work_capture_image, &mut temp_capture_image, &core::no_array()?)?;
    
            // プレイヤー毎の位置で処理する
            let (width, height) = (frame.cols(), frame.rows());
            let player_area_width = width / smashbros_data.player_count;
            let mut chara_name_list: Vec<String> = Default::default();
            let re = Regex::new(r"\s*(\w+)\s*").unwrap();
            for player_count in 0..smashbros_data.player_count {
                if !smashbros_data.character_name_list[player_count as usize].is_empty() {
                    // 既にプレイヤーキャラクターが確定しているならスキップ
                    continue;
                }
                // 高さそんなにいらないので適当に小さくする
                let mut player_name_area = core::Rect {
                    x: player_area_width*player_count +30, y: 0,        // 30:{N}P のプレイヤー表示の幅
                    width: player_area_width -10 -30, height: height/7  // 10:稲妻が処理後に黒四角形になって文字領域として誤認されるのを防ぐため
                };
                let mut name_area_image = core::Mat::roi(&temp_capture_image, player_name_area)?;
                let mut gray_name_area_image = core::Mat::roi(&work_capture_image, player_name_area)?;
    
                // 輪郭捕捉して
                let name_contour_image = Utils::trimming_any_rect(
                    &mut name_area_image, &gray_name_area_image, Some(5), None, None, false, None)?;
                Utils::cvt_color_to(&name_contour_image, &mut name_area_image, ColorFormat::RGB as i32)?;
                opencv::highgui::imshow(&format!("name{}", player_count), &name_area_image)?;
                
                // tesseract でキャラ名取得して, 余計な文字を排除
                let text = &async_std::task::block_on(Utils::run_ocr(&name_area_image)).unwrap();
                if let Some(caps) = re.captures( text ) {
                    chara_name_list.push( String::from(&caps[1]) );
                }
            }
    
            println!("captured name list: {:?}", chara_name_list);
            Ok(false)
        })?;

        if smashbros_data.character_name_list.is_empty() && self.buffer.is_replay_end() {
            smashbros_data.character_name_list.push("".to_string());
        }

        Ok(())
    }

    fn draw(&self, _capture: &mut core::Mat) {}
}
impl HamVsSpamScene {
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
            .set_border(0.98)
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

    /// このシーンだけ検出が厳しい。
    /// なぜか"GO"でなくて 時間の 00.00 で検出するという ("GO"はエフェクトかかりすぎて検出しづらかった
    /// ラグとかある状況も予想されるので、00.00 が検出できたら"GO"とでていなくとも次に遷移する
    /// 右上の 00.00 が表示されている場所に ある程度の確率で検出してればよしとする
    /// (背景がステージによって全然違うのでマスク処理するのが難しい)
    fn is_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<bool> {
        async_std::task::block_on(async {
            self.scene_judgment.match_captured_scene(&capture_image).await;
        });

        if 0.85 < self.scene_judgment.prev_match_ratio &&
            568 == self.scene_judgment.prev_match_point.x && 13 == self.scene_judgment.prev_match_point.y {
            return Ok(true);
        }

        Ok(false)
    }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }

    // now_scene が GameStart になることはない("GO"を検出した時はもう GamePlaying であるため)
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GamePlaying }

    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
    
    fn draw(&self, _capture: &mut core::Mat) {}
}

/// 試合中の検出
/// save: プレイヤー毎のストック(デカ[N - N]の画面の{N})
struct GamePlayingScene {
    black_scene_judgment: SceneJudgment,
    white_scene_judgment: SceneJudgment,
}
impl Default for GamePlayingScene {
    fn default() -> Self {
        Self {
            black_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/stock_hyphen_color_black.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/stock_hyphen_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            white_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/stock_hyphen_color_white.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/stock_hyphen_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
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

    fn is_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<bool> {
        async_std::task::block_on(async {
            self.black_scene_judgment.match_captured_scene(&capture_image).await;
            if self.black_scene_judgment.is_near_match() {
                return; // async-function
            }

            self.white_scene_judgment.match_captured_scene(&capture_image).await;
        });
        if self.black_scene_judgment.is_near_match() | self.white_scene_judgment.is_near_match() {
            println!("show N - N");
        }

        Ok(false)
    }

    // このシーンは [GameEnd] が検出されるまで待つ(つまり現状維持)
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GamePlaying }

    fn recoding_scene(&mut self, capture: &core::Mat) -> opencv::Result<()> {
        Ok(())
    }
    fn is_recoded(&self) -> bool {
        false
    }

    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }

    fn draw(&self, _capture: &mut core::Mat) {
        for scene_judgment in [&self.white_scene_judgment, &self.black_scene_judgment].iter_mut() {
            let size = scene_judgment.color_image.mat_size();
            let end_point = core::Point {
                x: scene_judgment.prev_match_point.x + size.get(1).unwrap() -1,
                y: scene_judgment.prev_match_point.y + size.get(0).unwrap() -1
            };
            imgproc::rectangle_points(_capture,
                scene_judgment.prev_match_point, end_point,
                core::Scalar::new(255.0, 0.0, 0.0, 0.0), 1, imgproc::LINE_8, 0).unwrap();
        }
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
            .set_border(0.98),
            time_up_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/time_up_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/time_up_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_border(0.98),
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

    fn is_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<bool> {
        // おそらく、ストック制を選択してる人のほうが多い(もしくは時間より先に決着がつくことが多い)
        async_std::task::block_on(async {
            self.game_set_scene_judgment.match_captured_scene(&capture_image).await;
            if self.game_set_scene_judgment.is_near_match() {
                return; // async-function
            }

            self.time_up_scene_judgment.match_captured_scene(&capture_image).await;
        });

        Ok(self.game_set_scene_judgment.is_near_match() | self.time_up_scene_judgment.is_near_match())
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GameEnd }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }

    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }

    fn draw(&self, _capture: &mut core::Mat) {}
}

/// 結果画面表示
/// save: プレイヤー毎の[戦闘力, 順位]
struct ResultScene {}
impl Default for ResultScene {
    fn default() -> Self {
        Self {}
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

    // 状態不明は他から遷移する、もしくは最初のシーンなので、自身ではならない
    fn is_scene(&mut self, _capture_image: &core::Mat) -> opencv::Result<bool> { Ok(false) }

    // 結果画面からは ReadyToFight の検出もあるけど、Dialog によって連戦が予想されるので Result へ
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::Result }

    fn recoding_scene(&mut self, capture: &core::Mat) -> opencv::Result<()> {
        Ok(())
    }
    fn is_recoded(&self) -> bool {
        false
    }

    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }

    fn draw(&self, _capture: &mut core::Mat) {}
}


/// 収集したデータ郡
pub struct SmashbrosData {
    // 基本データ
    pub player_count: i32,
    pub rule_name: String,
    pub start_time: std::time::Instant,
    pub end_time: std::time::Instant,

    // プレイヤーの数だけ存在するデータ
    pub character_name_list: Vec<String>,
    pub order_list: Vec<i32>,
    pub stock_list: Vec<i32>,
    pub group_list: Vec<String>,
}
impl Default for SmashbrosData {
    fn default() -> Self {
        Self {
            player_count: 1,
            rule_name: "unknown".to_string(),
            start_time: std::time::Instant::now(),
            end_time: std::time::Instant::now(),

            character_name_list: vec![],
            order_list: vec![],
            stock_list: vec![],
            group_list: vec![],
        }
    }
}


/// シーン全体を非同期で管理するクラス
pub struct SceneManager {
    capture: Box<dyn CaptureTrait>,
    scene_loading: LoadingScene,
    scene_list: Vec<Box<dyn SceneTrait + 'static>>,
    now_scene: SceneList,
    smashbros_data: SmashbrosData,
}
impl Default for SceneManager {
    fn default() -> Self {
        Self {
            //capture: Box::new(CaptureFromWindow::new("MonsterX U3.0R", "")),
            //capture: Box::new(CaptureFromVideoDevice::new(0)),
            capture: Box::new(CaptureFromDesktop::default()),
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
        }
    }
}
impl SceneManager {
    pub fn update(&mut self) -> opencv::Result<()> {
        let mut capture_image = self.capture.get_mat()?;

        if self.scene_loading.is_scene(&capture_image)? {
            // 読込中の画面(真っ黒に近い)はテンプレートマッチングで 1.0 がでてしまうので回避
            GUI::set_title(&format!("loading..."));
            return Ok(());
        }
        GUI::set_title(&format!(""));
        self.smashbros_data.start_time = std::time::Instant::now();

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
            if scene.is_scene(&capture_image)? {
                println!(
                    "[{:?}] match {:?} to {:?}",
                    SceneList::to_scene_list(scene.get_id()),
                    self.now_scene, scene.to_scene(self.now_scene)
                );
                
                self.now_scene = scene.to_scene(self.now_scene);
            }
        }

        opencv::highgui::imshow("capture image", &capture_image).unwrap();

        Ok(())
    }
}

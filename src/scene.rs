
use opencv::{
    core,
    imgcodecs,
    imgproc,
    prelude::*
};

use crate::capture::*;
use crate::gui::GUI;


macro_rules! measure {
    ( $x:expr) => {{
        let start = std::time::Instant::now();
        let result = $x;
        let end = start.elapsed();

        &format!("{}.{:03}s", end.as_secs(), end.subsec_nanos() / 1_000_000)
    }};
}

#[derive(Copy, Clone)]
pub enum ColorFormat {
    NONE = 0, GRAY = 1,
    RGB = 3, RGBA = 4,
}

pub struct Utils {
}
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

    /// src に対しての特定色を透過色とした mask を作成
    pub fn make_trans_mask_from_noalpha(src: &core::Mat, dst: &mut core::Mat) -> opencv::Result<()> {
        let trans_color = [0.0, 0.0, 0.0, 1.0];
        let lower_mat = core::Mat::from_slice(&trans_color)?;
        let upper_mat = core::Mat::from_slice(&trans_color)?;
        let mut mask = core::Mat::default()?;
        core::in_range(&src, &lower_mat, &upper_mat, &mut mask)?;
        core::bitwise_not(&mask, dst, &core::no_array()?)?;

        Ok(())
    }
}


/// シーン判定汎用クラス
struct SceneJudgment {
    color_image: core::Mat,
    mask_image: Option<core::Mat>,
    trans_mask_image: Option<core::Mat>,
    judgment_type: ColorFormat,
    prev_match_ratio: f64,
    prev_match_point: core::Point,
    border_match_ratio: f64,
}
impl Default for SceneJudgment {
    fn default() -> Self {
        Self {
            color_image: core::Mat::default().unwrap(),
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
        let mut converted_color_image = core::Mat::default()?;

        // 強制で color_format にする
        Utils::cvt_color_to(&color_image, &mut converted_color_image, color_format as i32)?;

        let converted_mask_image = match &mask_image {
            Some(v) => {
                let mut converted_mask_image = core::Mat::default()?;
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
        let mut converted_color_image = core::Mat::default()?;
        Utils::cvt_color_to(&color_image, &mut converted_color_image, ColorFormat::RGBA as i32)?;

        // 透過画像の場合は予め透過マスクを作成する
        let mut converted_mask_image = core::Mat::default()?;
        let mut trans_mask_image = core::Mat::default()?;
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
        let mut result = core::Mat::default().unwrap();
        let mut converted_captured_image = core::Mat::default().unwrap();
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

        core::min_max_loc(&result,
            &mut 0.0, &mut self.prev_match_ratio,
            &mut core::Point::default(), &mut self.prev_match_point,
            &core::no_array().unwrap()
        ).unwrap();
    }

    /// 前回のテンプレートマッチングで大体一致しているか
    fn is_near_match(&self) -> bool {
        self.border_match_ratio <= self.prev_match_ratio
    }
}

/// シーン雛形 (動作は子による)
trait SceneTrait {
    /// シーン識別ID
    fn get_id(&self) -> i32;
    /// シーンを検出するか
    fn continue_match(&self, now_scene: SceneList) -> bool;
    /// "この"シーンかどうか
    fn is_scene(&mut self, mat: &core::Mat) -> opencv::Result<bool>;
    /// 次はどのシーンに移るか
    fn to_scene(&self, now_scene: SceneList) -> SceneList;

    fn draw(&self, capture: &mut core::Mat);
}

use strum::IntoEnumIterator;
use strum_macros::EnumIter;
#[derive(Clone, Copy, Debug, EnumIter, PartialEq)]
enum SceneList {
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

/// 状態不明のシーン
#[derive(Default)]
struct UnknownScene {}
impl SceneTrait for UnknownScene {
    fn get_id(&self) -> i32 { SceneList::Unknown as i32 }

    // 状態不明は他から遷移する、もしくは最初のシーンなので、自身ではならない、他に移らない
    fn continue_match(&self, _now_scene: SceneList) -> bool { false }
    fn is_scene(&mut self, _capture_image: &core::Mat) -> opencv::Result<bool> { Ok(false) }
    fn to_scene(&self, now_scene: SceneList) -> SceneList { now_scene }
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

    fn draw(&self, _capture: &mut core::Mat) {}
}

/// Ready to Fight が表示されているシーン]
/// スマブラがちゃんとキャプチャされているかで使用
struct ReadyToFightScene {
    grad_scene_judgment: SceneJudgment,
    red_scene_judgment: SceneJudgment,
}
impl Default for ReadyToFightScene {
    fn default() -> Self {
        Self {
            grad_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/ready_to_fight_color_0.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/ready_to_fight_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            red_scene_judgment: SceneJudgment::new_gray(
                imgcodecs::imread("resource/ready_to_fight_color_1.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/ready_to_fight_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
        }
    }
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

    fn draw(&self, _capture: &mut core::Mat) {
        let size = self.red_scene_judgment.color_image.mat_size();
        let end_point = core::Point {
            x: self.red_scene_judgment.prev_match_point.x + size.get(1).unwrap() -1,
            y: self.red_scene_judgment.prev_match_point.y + size.get(0).unwrap() -1
        };
        imgproc::rectangle_points(_capture,
            self.red_scene_judgment.prev_match_point, end_point,
            core::Scalar::new(0.0, 255.0, 0.0, 0.0), 1, imgproc::LINE_8, 0).unwrap();
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
            scene_judgment: SceneJudgment::new_trans(
                imgcodecs::imread("resource/ready_ok_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/ready_ok_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_border(0.96)
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

    fn draw(&self, _capture: &mut core::Mat) {}
}

/// キャラクターが大きく表示されてる画面
/// save: キャラクターの種類, ルール(time | stock), 時間
struct HamVsSpamScene {
    scene_judgment: SceneJudgment,
}
impl Default for HamVsSpamScene {
    fn default() -> Self {
        Self {
            scene_judgment: SceneJudgment::new_trans(
                imgcodecs::imread("resource/vs_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/vs_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_border(0.99)
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

        Ok(self.scene_judgment.is_near_match())
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::HamVsSpam }

    fn draw(&self, _capture: &mut core::Mat) {}
}

/// 試合開始の検出
/// なぜか"GO"でなくて 時間の 00.00 で検出するという ("GO"はエフェクトかかりすぎて検出しづらかったｗ
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

    fn is_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<bool> {
        async_std::task::block_on(async {
            self.scene_judgment.match_captured_scene(&capture_image).await;
        });

        if 0.85 < self.scene_judgment.prev_match_ratio && 568 == self.scene_judgment.prev_match_point.x && 13 == self.scene_judgment.prev_match_point.y {
            return Ok(true);
        }

        Ok(self.scene_judgment.is_near_match())
    }

    // now_scene が GameStart になることはない("GO"を検出した時はもう GamePlaying であるため)
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GamePlaying }

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

        GUI::set_title(&format!("{:2.3} {}x{}",
            self.white_scene_judgment.prev_match_ratio,
            self.white_scene_judgment.prev_match_point.x, self.white_scene_judgment.prev_match_point.y
        ));

        Ok(false)
    }

    // このシーンは [GameEnd] が検出されるまで待つ(つまり現状維持)
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GamePlaying }

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

    fn draw(&self, _capture: &mut core::Mat) {}
}


/* シーン全体を非同期で管理するクラス */
pub struct SceneManager {
    scene_loading: LoadingScene,
    scene_list: Vec<Box<dyn SceneTrait + 'static>>,
    now_scene: SceneList,
    capture: Box<dyn Capture>,
    matching_wait: i32,
}
impl Default for SceneManager {
    fn default() -> Self {
        Self {
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
            now_scene: SceneList::Unknown,
            //capture: Box::new(CaptureFromWindow::new("MonsterX U3.0R", "")),
            capture: Box::new(CaptureFromVideoDevice::new(0)),
            matching_wait: 0,
        }
    }
}
impl SceneManager {
    pub fn update(&mut self) -> opencv::Result<()> {
        let mut capture_image = self.capture.get_mat()?;

        // TODO:テンプレートマッチングが遅い場合があるので、遅延をかける
        self.matching_wait += 1;
        if 0 < self.matching_wait % 5 {
            return Ok(());
        }

        if self.scene_loading.is_scene(&capture_image)? {
            // 読込中の画面(真っ黒に近い)はテンプレートマッチングで 1.0 がでてしまうので回避
            GUI::set_title(&format!("loading..."));
            return Ok(());
        }
        GUI::set_title(&format!(""));

        // 現在キャプチャと比較して遷移する
        for scene in &mut self.scene_list[..] {
            scene.draw(&mut capture_image);

            if !scene.continue_match(self.now_scene) {
                continue;
            }

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


use opencv::{
    core,
    imgcodecs,
    imgproc,
    prelude::*
};

use crate::capture::*;
use crate::gui::GUI;


/* シーン判定汎用クラス */
struct SceneJudgment {
    color_image: core::Mat,
    mask_image: Option<core::Mat>,
    trans_mask_image: Option<core::Mat>,
    is_trans: bool,
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
            is_trans: false,
            border_match_ratio: 0.99,
            prev_match_ratio: 0f64,
            prev_match_point: Default::default(),
        }
    }
}
impl SceneJudgment {
    // 普通のRGB画像と一致するシーン
    fn new(color_image: core::Mat, mask_image: Option<core::Mat>) -> opencv::Result<Self> {
        Ok(Self {
            color_image: color_image, mask_image: mask_image,
            trans_mask_image: None,
            is_trans: false,
            ..Default::default()
        })
    }
    // 透過画像と一致するシーン
    fn new_trans(color_image: core::Mat, mask_image: Option<core::Mat>) -> opencv::Result<Self> {
        // 他のメンバを透過に変換しておく
        let mut converted_color_image = color_image.clone();
        if color_image.channels()? == 3 {
            imgproc::cvt_color(&color_image, &mut converted_color_image, imgproc::COLOR_RGB2RGBA, 0)?;
        }

        // 透過画像の場合は予め透過マスクを作成する
        let mut converted_mask_image = core::Mat::default()?;
        let mut trans_mask_image = core::Mat::default()?;
        match &mask_image {
            Some(v) => {
                if v.channels()? == 3 {
                    // RGB の場合は RGBA にする
                    imgproc::cvt_color(&v, &mut trans_mask_image, imgproc::COLOR_RGB2RGBA, 0)?;
                    imgproc::cvt_color(&v, &mut converted_mask_image, imgproc::COLOR_RGB2RGBA, 0)?;
                } else {
                    trans_mask_image = v.clone();
                    converted_mask_image = v.clone();
                }
            },
            None => {
                SceneJudgment::_make_trans_mask_from_noalpha(
                    &color_image, &mut trans_mask_image)?;
            },
        }

        Ok(Self {
            color_image: converted_color_image, mask_image: Some(converted_mask_image),
            trans_mask_image: Some(trans_mask_image),
            is_trans: true,
            ..Default::default()
        })
    }

    // src に対しての特定色を透過色とした mask を作成
    fn _make_trans_mask_from_noalpha(src: &core::Mat, dst: &mut core::Mat) -> opencv::Result<()> {
        let trans_color = [0.0, 0.0, 0.0, 1.0];
        let lower_mat = core::Mat::from_slice(&trans_color)?;
        let upper_mat = core::Mat::from_slice(&trans_color)?;
        let mut mask = core::Mat::default()?;
        core::in_range(&src, &lower_mat, &upper_mat, &mut mask)?;
        core::bitwise_not(&mask, dst, &core::no_array()?)?;

        Ok(())
    }

    // キャプチャされた画像とシーンとをテンプレートマッチングして、一致した確率と位置を返す
    fn match_captured_scene(&mut self, captured_image: &core::Mat) -> opencv::Result<(f64, core::Point)> {
        let mut result = core::Mat::default()?;
        let mut converted_captured_image = core::Mat::default()?;
        captured_image.copy_to(&mut converted_captured_image)?;

        if self.is_trans {
            // 透過画像の場合
            // is_trans が true の時はそもそも None の状態になることはない
            if let Some(v) = &self.trans_mask_image {
                imgproc::match_template(&self.color_image, &converted_captured_image, &mut result,
                    imgproc::TM_CCOEFF_NORMED, &v)?;
            }
        } else {
            // 透過画像でない場合
            // None の場合は converted_captured_image はコピーされた状態だけでよい
            if let Some(v) = &self.mask_image {
                // captured_image を mask_image で篩いにかけて,無駄な部分を削ぐ
                core::bitwise_and(&captured_image, &v,
                    &mut converted_captured_image, &mut core::no_array()?)?;
            }
            imgproc::match_template(&self.color_image, &converted_captured_image, &mut result,
                imgproc::TM_CCOEFF_NORMED, &core::no_array()?)?;
        }

        core::min_max_loc(&result,
            &mut 0.0, &mut self.prev_match_ratio,
            &mut core::Point::default(), &mut self.prev_match_point,
            &core::no_array()?
        )?;

        Ok((self.prev_match_ratio, self.prev_match_point))
    }

    // 前回のテンプレートマッチングで大体一致しているか
    fn is_near_match(&self) -> bool {
        self.border_match_ratio <= self.prev_match_ratio
    }
}

/* シーン(動作は子による) */
trait Scene {
    // "こ"のシーンかどうか
    fn is_scene(&mut self, mat: &core::Mat) -> opencv::Result<bool>;
}

/* 状態不明のシーン */
#[derive(Default)]
struct SceneUnknown {}
impl Scene for SceneUnknown {
    // 状態不明は他から遷移する、もしくは最初のシーンなので、自身ではならない
    fn is_scene(&mut self, _mat: &core::Mat) -> opencv::Result<bool> { Ok(false) }
}

/* ダイアログが表示されているシーン */
struct SceneDialog {
    scene_judgment: SceneJudgment,
}
impl Default for SceneDialog {
    fn default() -> Self {
        Self {
            scene_judgment: SceneJudgment::new(
                imgcodecs::imread("resource\\battle_retry_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource\\battle_retry_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
        }
    }
}
impl Scene for SceneDialog {
    fn is_scene(&mut self, mat: &core::Mat) -> opencv::Result<bool> {
        self.scene_judgment.match_captured_scene(&mat);
        Ok(self.scene_judgment.is_near_match())
    }
}


/* Ready to Fight が表示されているシーン */
struct SceneReadyToFight {
    grad_scene_judgment: SceneJudgment,
    red_scene_judgment: SceneJudgment,
}
impl Default for SceneReadyToFight {
    fn default() -> Self {
        Self {
            grad_scene_judgment: SceneJudgment::new_trans(
                imgcodecs::imread("resource\\ready_to_fight_color_0.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource\\ready_to_fight_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            red_scene_judgment: SceneJudgment::new_trans(
                imgcodecs::imread("resource\\ready_to_fight_color_1.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource\\ready_to_fight_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
        }
    }
}
impl Scene for SceneReadyToFight {
    fn is_scene(&mut self, mat: &core::Mat) -> opencv::Result<bool> {
        // 多分 red版 ReadyToFight のほうが多いので先にする
        self.red_scene_judgment.match_captured_scene(&mat)?;
        if self.red_scene_judgment.is_near_match() {
            return Ok(true);
        }

        self.grad_scene_judgment.match_captured_scene(&mat)?;
        Ok(self.grad_scene_judgment.is_near_match())
    }
}

/* シーン全体を非同期で管理するクラス */
pub struct SceneManager {
    scene_list: Vec<Box<dyn Scene>>,
    capture: Box<dyn Capture>,
}
impl Default for SceneManager {
    fn default() -> Self {
        Self {
            scene_list: vec![
                Box::new(SceneUnknown::default()),
                Box::new(SceneDialog::default()),
            ],
            capture: Box::new(CaptureFromWindow::new("MonsterX U3.0R", "")),
        }
    }
}
impl SceneManager {
    pub fn update(&mut self) -> opencv::Result<()> {
        let capture_image = self.capture.get_mat()?;

        opencv::highgui::imshow("captured", &capture_image)?;

        for scene in &mut self.scene_list[..] {
            scene.is_scene(&capture_image)?;
        }

        Ok(())
    }
}

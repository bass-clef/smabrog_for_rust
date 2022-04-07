use super::*;

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
            self.scene_judgment.match_captured_scene(&capture_image).await
        })?;

        Ok(self.scene_judgment.is_near_match())
    }

    // このシーンからは複数の遷移があるので、現状維持
    fn to_scene(&self, now_scene: SceneList) -> SceneList { now_scene }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }

    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}

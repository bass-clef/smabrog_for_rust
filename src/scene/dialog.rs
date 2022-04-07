use super::*;

/// ダイアログが表示されているシーン
/// 突然の回線切断とか、連続して試合をするとき、録画のYボタンを押したとき、など
pub struct DialogScene {
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
            self.scene_judgment.match_captured_scene(&capture_image).await
        })?;

        Ok(self.scene_judgment.is_near_match())
    }

    // このシーンからは複数の遷移があるけど、表示された後は常に最初に戻る
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::Unknown }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }

    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}

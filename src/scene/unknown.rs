use super::*;

/// 状態不明のシーン
#[derive(Default)]
pub struct UnknownScene {}
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

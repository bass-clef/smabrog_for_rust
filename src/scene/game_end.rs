use super::*;

/// 試合終わりの検出 ("GAME SET" or "TIME UP")
pub struct GameEndScene {
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
            self.game_set_scene_judgment.match_captured_scene(&capture_image).await?;
            if self.game_set_scene_judgment.is_near_match() {
                return Ok(()); // async-function
            }

            self.time_up_scene_judgment.match_captured_scene(&capture_image).await
        })?;

        Ok(self.game_set_scene_judgment.is_near_match() || self.time_up_scene_judgment.is_near_match())
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GameEnd }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}

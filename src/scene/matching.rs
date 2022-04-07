use super::*;

/// マッチング中の画面 (CPUと戦えるあの画面)
/// save: プレイヤー人数(2p, 4p)
pub struct MatchingScene {
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
            // self.scene_judgment_smash_tournament.match_captured_scene(&capture_image).await?;
            // if self.scene_judgment_smash_tournament.is_near_match() {
            //     return;
            // }

            self.scene_judgment_ooo_tournament.match_captured_scene(&capture_image).await?;
            if self.scene_judgment_ooo_tournament.is_near_match() {
                return Ok(());
            }

            self.scene_judgment_with4.match_captured_scene(&capture_image).await?;
            if self.scene_judgment_with4.is_near_match() {
                return Ok(());
            }

            self.scene_judgment.match_captured_scene(&capture_image).await
        })?;

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

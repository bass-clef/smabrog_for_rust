use super::*;

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
            self.grad_scene_judgment.match_captured_scene(&capture_image).await?;
            if self.grad_scene_judgment.is_near_match() {
                return Ok(()); // async-function
            }

            self.red_scene_judgment.match_captured_scene(&capture_image).await
        })?;
        
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

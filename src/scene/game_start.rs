use super::*;

/// 試合開始の検出
pub struct GameStartScene {
    scene_judgment: SceneJudgment,
    is_scene: bool,
}
impl Default for GameStartScene {
    fn default() -> Self {
        Self {
            scene_judgment: SceneJudgment::new(
                    imgcodecs::imread("resource/battle_time_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/battle_time_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.90),
            is_scene: false,
        }
    }
}
impl SceneTrait for GameStartScene {
    fn get_id(&self) -> i32 { SceneList::GameStart as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> { Some(&self.scene_judgment) }
    
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
            self.scene_judgment.match_captured_scene(&capture_image).await
        })?;

        if self.scene_judgment.is_near_match() {
            if !self.is_scene {
                self.is_scene = true;
                log::info!("[GameStartBegin]({:2.3}%)", self.scene_judgment.prev_match_ratio);
            }
            if let Some(smashbros_data) = smashbros_data {
                self.captured_count_down(capture_image, smashbros_data)?;
            }
        } else if self.is_scene {
            // N:00 の状態はまだ始まっていないので、違ってくる時に次のシーンに遷移する
            self.is_scene = false;
            return Ok(true);
        }

        Ok(false)
    }

    // now_scene が GameStart になることはない("GO"を検出した時はもう GamePlaying であるため)
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GamePlaying }

    fn recoding_scene(&mut self, _capture: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}
impl GameStartScene {
    // カウントダウン が検出されているフレームの処理
    fn captured_count_down(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        self.captured_bgm_name(capture_image, smashbros_data)?;

        Ok(())
    }

    // BGM が検出されているフレームを処理
    fn captured_bgm_name(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let mut bgm_capture_image = core::Mat::roi(capture_image, core::Rect::new(18, 30, 240, 18))?;

        // 近似白黒処理して
        let mut gray_capture_image = core::Mat::default();
        imgproc::threshold(&bgm_capture_image, &mut gray_capture_image, 150.0, 255.0, imgproc::THRESH_BINARY)?;
        let mut work_capture_image = core::Mat::default();
        utils::cvt_color_to(&gray_capture_image, &mut work_capture_image, ColorFormat::GRAY as i32)?;

        // opencv::highgui::imshow("gray_capture_image", &gray_capture_image)?;

        // 輪郭捕捉して
        let work_capture_image = utils::trimming_any_rect(
            &mut bgm_capture_image, &work_capture_image, Some(5), Some(0.0), None, true, Some(core::Scalar::new(128.0, 128.0, 128.0, 0.0)))?;

        // 白黒反転して
        core::bitwise_not(&work_capture_image, &mut gray_capture_image, &core::no_array())?;
        let mut work_capture_image = core::Mat::default();
        utils::cvt_color_to(&gray_capture_image, &mut work_capture_image, ColorFormat::GRAY as i32)?;

        // opencv::highgui::imshow("bgm_capture_image", &work_capture_image)?;

        // tesseract で文字列を取得して, 余計な文字を排除
        let bgm_text = &async_std::task::block_on(utils::run_ocr_with_japanese(&work_capture_image)).unwrap().to_string();
        if bgm_text.is_empty() {
            return Ok(());
        }
        let bgm_text = bgm_text.replace(" ", "");

        smashbros_data.guess_bgm_name(bgm_text);

        Ok(())
    }
}

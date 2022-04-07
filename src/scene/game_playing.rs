use super::*;

/// 試合中の検出
/// save: プレイヤー毎のストック(デカ[N - N]の画面の{N})
pub struct GamePlayingScene {
    stock_black_scene_judgment: SceneJudgment,
    stock_white_scene_judgment: SceneJudgment,
    stock_number_mask: core::Mat,
}
impl Default for GamePlayingScene {
    fn default() -> Self {
        Self {
            stock_black_scene_judgment: SceneJudgment::new_gray(
                    imgcodecs::imread("resource/stock_hyphen_color_black.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/stock_hyphen_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_size(core::Rect{    // 参照される回数が多いので matchTemplate する大きさ減らす
                    x:0, y:100, width:640, height: 100
                })
                .set_border(0.95),
            stock_white_scene_judgment: SceneJudgment::new_gray(
                    imgcodecs::imread("resource/stock_hyphen_color_white.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/stock_hyphen_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_size(core::Rect{
                    x:0, y:100, width:640, height: 100
                })
                .set_border(0.95),
            stock_number_mask: imgcodecs::imread("resource/stock_number_mask.png", imgcodecs::IMREAD_GRAYSCALE).unwrap()
        }
    }
}
impl SceneTrait for GamePlayingScene {
    fn get_id(&self) -> i32 { SceneList::GamePlaying as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> {
        // 高い方を返す
        if self.stock_black_scene_judgment.prev_match_ratio < self.stock_white_scene_judgment.prev_match_ratio {
            return Some(&self.stock_white_scene_judgment);
        }

        Some(&self.stock_black_scene_judgment)
    }
    
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::GamePlaying => true,
            _ => false,
        }
    }

    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        let smashbros_data = match smashbros_data {
            Some(smashbros_data) => smashbros_data,
            None => return Ok(false),
        };

        match smashbros_data.get_player_count() {
            2 => self.game_playing_with_2(capture_image, smashbros_data),
            4 => self.game_playing_with_4(capture_image, smashbros_data),
            _ => Ok(false) // TODO?: 8 人対戦とか?
        }
    }

    // このシーンは [GameEnd] が検出されるまで待つ(つまり現状維持)
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::GamePlaying }

    fn recoding_scene(&mut self, _capture_image: &core::Mat) -> opencv::Result<()> { Ok(()) }
    fn is_recoded(&self) -> bool { false }
    fn detect_data(&mut self, _smashbros_data: &mut SmashbrosData) -> opencv::Result<()> { Ok(()) }
}
impl GamePlayingScene {
    // 1 on 1
    fn game_playing_with_2(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        match smashbros_data.get_rule() {
            BattleRule::Stock | BattleRule::Stamina => {
                self.stock_scene_judgment(capture_image, smashbros_data)?;
            },
            _ => (),
        }
        Ok(false)
    }
    // smash
    fn game_playing_with_4(&mut self, _capture_image: &core::Mat, _smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        Ok(false)
    }

    // ストックを検出
    fn stock_scene_judgment(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        if smashbros_data.all_decided_stock() {
            // すべてのプレイヤーが確定している場合は判定すら行わない (matchTemaplte は処理コストが高い)
            return Ok(false);
        }

        async_std::task::block_on(async {
            self.stock_black_scene_judgment.match_captured_scene(&capture_image).await?;
            if self.stock_black_scene_judgment.is_near_match() {
                return Ok(()); // async-function
            }
            
            self.stock_white_scene_judgment.match_captured_scene(&capture_image).await
        })?;

        if self.stock_black_scene_judgment.is_near_match() || self.stock_white_scene_judgment.is_near_match() {
            Self::captured_stock_number(&capture_image, smashbros_data, &self.stock_number_mask)?;
        }

        Ok(false)
    }

    // ストックが検出されているフレームを処理
    pub fn captured_stock_number(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, stock_number_mask: &core::Mat) -> opencv::Result<bool> {
        use regex::Regex;
        // ストックの位置を切り取って
        let mut temp_capture_image = core::Mat::default();
        let mut gray_number_area_image = core::Mat::default();
        utils::cvt_color_to(&capture_image, &mut gray_number_area_image, ColorFormat::GRAY as i32)?;
        core::bitwise_and(&gray_number_area_image, stock_number_mask, &mut temp_capture_image, &core::no_array())?;

        // 近似白黒処理して
        let mut work_capture_image = core::Mat::default();
        imgproc::threshold(&temp_capture_image, &mut work_capture_image, 250.0, 255.0, imgproc::THRESH_BINARY)?;
        core::bitwise_and(&gray_number_area_image, &work_capture_image, &mut temp_capture_image, &core::no_array())?;
        core::bitwise_not(&temp_capture_image, &mut work_capture_image, &core::no_array())?;

        // プレイヤー毎に処理する
        let (width, height) = (capture_image.cols(), capture_image.rows());
        let player_area_width = width / smashbros_data.get_player_count();
        let re = Regex::new(r"\s*(\d)\s*").unwrap();
        let mut skip_count = 0;
        for player_number in 0..smashbros_data.get_player_count() {
            if smashbros_data.is_decided_stock(player_number) {
                // 既にプレイヤーのストックが確定しているならスキップ
                skip_count += 1;
                continue;
            }
            // 適当に小さくする
            let player_stock_area = core::Rect {
                x: player_area_width*player_number, y: height/4, width: player_area_width, height: height/2
            };
            let mut stock_area_image = core::Mat::roi(&work_capture_image, player_stock_area)?;
            let gray_stock_area_image = core::Mat::roi(&gray_number_area_image, player_stock_area)?;

            // 輪郭捕捉して
            let stock_contour_image = utils::trimming_any_rect(
                &mut stock_area_image, &gray_stock_area_image, Some(5), Some(1000.0), None, true, None)?;

            // tesseract で文字(数値)を取得して, 余計な文字を排除
            let number = &async_std::task::block_on(utils::run_ocr_with_number(&stock_contour_image, Some("123"), true)).unwrap().to_string();
            if let Some(caps) = re.captures( number ) {
                let number = (&caps[1]).parse().unwrap_or(-1);
                smashbros_data.guess_stock(player_number, number);
            }
        }

        Ok(smashbros_data.get_player_count() == skip_count)
    }
}

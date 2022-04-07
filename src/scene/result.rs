use super::*;

/// 結果画面表示
/// save: プレイヤー毎の[戦闘力, 順位]
pub struct ResultScene {
    pub buffer: CaptureFrameStore,
    scene_judgment_list: Vec<SceneJudgment>,
    count_down_scene_judgment: SceneJudgment,
    retry_battle_scene_judgment: SceneJudgment,
    result_power_mask: core::Mat
}
impl Default for ResultScene {
    fn default() -> Self {
        let mut scene_judgment_list = vec![];
        for player_number in 1..=4 {
            let path = format!("resource/result_player_order_{}_", player_number);
            scene_judgment_list.push(
                SceneJudgment::new_trans(
                    imgcodecs::imread(&(path.clone() + "color.png"), imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread(&(path + "mask.png"), imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.985)
            );
        }

        Self {
            scene_judgment_list: scene_judgment_list,
            count_down_scene_judgment: SceneJudgment::new(
                imgcodecs::imread("resource/result_time_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/result_time_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_border(0.90),
            retry_battle_scene_judgment: SceneJudgment::new(
                imgcodecs::imread("resource/battle_retry_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/battle_retry_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            buffer: CaptureFrameStore::default(),
            result_power_mask: imgcodecs::imread("resource/result_power_mask.png", imgcodecs::IMREAD_GRAYSCALE).unwrap(),
        }
    }
}
impl SceneTrait for ResultScene {
    fn get_id(&self) -> i32 { SceneList::Result as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> { Some(&self.count_down_scene_judgment) }

    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::GameEnd => true,
            _ => false,
        }
    }

    // ResultScene の後に検出するシーンがないので、結果の検出だけ行う
    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        async_std::task::block_on(async {
            self.count_down_scene_judgment.match_captured_scene(capture_image).await
        })?;
        if !self.count_down_scene_judgment.is_near_match() {
            return Ok(false);
        }

        let smashbros_data = match smashbros_data {
            Some(smashbros_data) => smashbros_data,
            None => return Ok(false),
        };

        match smashbros_data.get_player_count() {
            2 => self.result_with_2(capture_image, smashbros_data),
            4 => self.result_with_4(capture_image, smashbros_data),
            _ => Ok(false) // TODO?: 8 人対戦とか?, 3人もあるらしい…
        }
    }

    // 結果画面からは ReadyToFight の検出もあるけど、Dialog によって連戦が予想されるので Result へ
    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::Result }

    fn recoding_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<()> {
        async_std::task::block_on(async {
            self.retry_battle_scene_judgment.match_captured_scene(capture_image).await
        })?;
        if self.retry_battle_scene_judgment.is_near_match() {
            // 「同じ相手との再戦を希望しますか？」のダイアログに一致してしまうと誤検出するので、そのフレームだけダミーの Mat を渡す
            self.buffer.recoding_frame(&core::Mat::default())
        } else {
            self.buffer.recoding_frame(capture_image)
        }
    }
    fn is_recoded(&self) -> bool { self.buffer.is_filled() }
    fn detect_data(&mut self, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let result_power_mask = &self.result_power_mask;
        let scene_judgment_list = &mut self.scene_judgment_list;
        self.buffer.replay_frame(|frame| {
            Self::captured_order(&frame, smashbros_data, scene_judgment_list)?;
            Self::captured_power(&frame, smashbros_data, result_power_mask)?;

            Ok(false)
        })?;
        Ok(())
    }
}
impl ResultScene {
    const ORDER_AREA_POS: [[core::Point; 4]; 2] = [
        [core::Point{x:205, y:4}, core::Point{x:470, y:4}, core::Point{x:0, y:0}, core::Point{x:0, y:0}],
        [core::Point{x: 90, y:0}, core::Point{x:250, y:0}, core::Point{x:420, y:0}, core::Point{x:580, y:0}]
    ];

    // 1 on 1
    fn result_with_2(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        self.result_scene_judgment(capture_image, smashbros_data)?;
        Ok(false)
    }
    // smash
    fn result_with_4(&mut self, _capture_image: &core::Mat, _smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        Ok(false)
    }

    // 結果画面を検出, retry_battle_scene_judgment の精度がよくなったので、検出出来る時にくる
    fn result_scene_judgment(&mut self, capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        if smashbros_data.all_decided_result() {
            // すべてのプレイヤーが確定している場合は判定すら行わない (matchTemaplte は処理コストが高い)
            return Ok(false);
        }
        if self.buffer.is_filled() {
            if !self.buffer.is_replay_end() {
                return Ok(false);
            }
        }

        if !self.buffer.is_recoding_started() {
            self.buffer.start_recoding_by_time(std::time::Duration::from_secs(3));
            self.buffer.recoding_frame(capture_image)?;
        }

        if self.scene_judgment_list.iter().any( |scene_judgment| scene_judgment.is_near_match() ) {
            // 順位の判定はそのフレームがほしいので、0フレーム目から録画をする
            self.buffer.start_recoding_by_time(std::time::Duration::from_secs(3));
            self.buffer.recoding_frame(capture_image)?;
        }

        Ok(false)
    }

    /// 順位が検出されているフレームの処理
    pub fn captured_order(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, scene_judgment_list: &mut Vec<SceneJudgment>) -> opencv::Result<bool> {
        let index_by_player_max = smashbros_data.get_player_count()/2-1;
        for player_number in 0..smashbros_data.get_player_count() {
            let order_number_pos = &Self::ORDER_AREA_POS[index_by_player_max as usize][player_number as usize];
            let order_number_area_image = core::Mat::roi(&capture_image.clone(),
                core::Rect{x:order_number_pos.x, y:order_number_pos.y, width:80, height:80})?;

            for order_count in 0..smashbros_data.get_player_count() {
                let scene_judgment = &mut scene_judgment_list[order_count as usize];
                async_std::task::block_on(async {
                    scene_judgment.match_captured_scene(&order_number_area_image).await
                })?;
                if scene_judgment.is_near_match() {
                    smashbros_data.guess_order(player_number, order_count+1);
                }
            }
        }

        Ok(smashbros_data.all_decided_order())
    }

    // 戦闘力が検出されているフレームの処理
    pub fn captured_power(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, result_power_mask: &core::Mat) -> opencv::Result<bool> {
        use regex::Regex;
        // 戦闘力の位置を切り取って
        let mut temp_capture_image = core::Mat::default();
        let mut gray_number_area_image = core::Mat::default();
        utils::cvt_color_to(&capture_image, &mut gray_number_area_image, ColorFormat::GRAY as i32)?;
        core::bitwise_and(&gray_number_area_image, result_power_mask, &mut temp_capture_image, &core::no_array())?;

        // プレイヤー毎に処理する
        let (width, height) = (capture_image.cols(), capture_image.rows());
        let player_area_width = width / smashbros_data.get_player_count();
        let re = Regex::new(r"[^\d]+").unwrap();
        for player_number in 0..smashbros_data.get_player_count() {
            // 適当に小さくする
            let player_power_area = core::Rect {
                x: player_area_width*player_number, y: height/4, width: player_area_width, height: height/2
            };
            let mut power_area_image = core::Mat::roi(&temp_capture_image, player_power_area)?;
            let gray_power_area_image = core::Mat::roi(&gray_number_area_image, player_power_area)?;

            // 輪郭捕捉して(maskで切り取った戦闘力の領域)
            let mut power_contour_image = utils::trimming_any_rect(
                &mut power_area_image, &gray_power_area_image, None, None, None, false, None)?;

            // 近似白黒処理して
            let mut work_capture_image = core::Mat::default();
            imgproc::threshold(&power_contour_image, &mut work_capture_image, 200.0, 255.0, imgproc::THRESH_BINARY)?;

            // 輪郭捕捉して(数値の範囲)
            let power_contour_image = utils::trimming_any_rect(
                &mut power_contour_image, &work_capture_image, Some(1), Some(1.0), None, false, None)?;
            utils::cvt_color_to(&power_contour_image, &mut power_area_image, ColorFormat::RGB as i32)?;

            // tesseract で文字(数値)を取得して, 余計な文字を排除
            let text = &async_std::task::block_on(utils::run_ocr_with_number(&power_area_image, Some("0123456789"), false)).unwrap().to_string();
            let number = re.split(text).collect::<Vec<&str>>().join("");
            smashbros_data.guess_power( player_number, number.parse().unwrap_or(-1) );
        }

        Ok(smashbros_data.all_decided_power())
    }
}

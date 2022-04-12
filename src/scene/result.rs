use super::*;

/// 結果画面表示
/// save: プレイヤー毎の[戦闘力, 順位]
pub struct ResultScene {
    pub buffer: CaptureFrameStore,
    scene_judgment_list: Vec<SceneJudgment>,
    count_down_scene_judgment: SceneJudgment,
    result_stock_color: SceneJudgment,
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
            result_stock_color: SceneJudgment::new(
                imgcodecs::imread("resource/result_stock_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/result_stock_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap()
            .set_border(0.95),
            retry_battle_scene_judgment: SceneJudgment::new(
                imgcodecs::imread("resource/battle_retry_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                Some(imgcodecs::imread("resource/battle_retry_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
            ).unwrap(),
            buffer: CaptureFrameStore::default()
                .set_file_name("result.avi".to_string()),
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
        let result_stock_color = &mut self.result_stock_color;
        self.buffer.replay_frame(|frame| {
            Self::captured_order(&frame, smashbros_data, scene_judgment_list)?;
            Self::captured_power(&frame, smashbros_data, result_power_mask)?;
            Self::capture_result_stock(&frame, smashbros_data, result_stock_color)?;

            Ok(false)
        })?;
        Ok(())
    }
}
impl ResultScene {
    // 順位の検出位置 [c2[1, 2, _, _], c4[1, 2, 3, 4]]
    const ORDER_AREA_POS: [[core::Point; 4]; 2] = [
        [core::Point{x:205, y:4}, core::Point{x:470, y:4}, core::Point{x:0, y:0}, core::Point{x:0, y:0}],
        [core::Point{x: 90, y:0}, core::Point{x:250, y:0}, core::Point{x:420, y:0}, core::Point{x:580, y:0}]
    ];

    // ストックの検出位置 [c2[1[KO, Fall, SD], 2[...], _[_], _[_]], c4[_]]
    const STOCK_AREA_POS: [[[core::Point; 3]; 4]; 2] = [
        [
            [core::Point{x:240, y:248}, core::Point{x:240, y:277}, core::Point{x:240, y:304}],
            [core::Point{x:504, y:248}, core::Point{x:504, y:277}, core::Point{x:504, y:304}],
            [core::Point{x:0, y:0}, core::Point{x:0, y:0}, core::Point{x:0, y:0}],
            [core::Point{x:0, y:0}, core::Point{x:0, y:0}, core::Point{x:0, y:0}]
        ],
        [
            [core::Point{x:0, y:0}, core::Point{x:0, y:0}, core::Point{x:0, y:0}],
            [core::Point{x:0, y:0}, core::Point{x:0, y:0}, core::Point{x:0, y:0}],
            [core::Point{x:0, y:0}, core::Point{x:0, y:0}, core::Point{x:0, y:0}],
            [core::Point{x:0, y:0}, core::Point{x:0, y:0}, core::Point{x:0, y:0}]
        ]
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
            let text = &async_std::task::block_on(utils::run_ocr_with_number(&power_area_image, Some("0123456789"), false)).unwrap();
            let number = re.split(text).collect::<Vec<&str>>().join("").parse().unwrap_or(-1);
            smashbros_data.guess_power( player_number, number );
        }

        Ok(smashbros_data.all_decided_power())
    }

    // 最終ストックが検出されているフレームの処理
    pub fn capture_result_stock(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, result_stock_color: &mut SceneJudgment) -> opencv::Result<bool> {
        if !smashbros_data.all_decided_max_stock() {
            return Ok(false);
        }

        // このフレームは 1 回くるか来ないかくらいでしか無いので、検出はあまり期待はしない
        async_std::task::block_on(async {
            result_stock_color.match_captured_scene(&capture_image).await
        })?;
        if !result_stock_color.is_near_match() {
            return Ok(false);
        }

        use regex::Regex;
        let re = Regex::new(r"[^\d]+").unwrap();
        let mut work_capture_image = core::Mat::default();
        let mut capture_stock = |stock_number_area_image: &mut core::Mat| -> opencv::Result<i32> {
            // 白黒にして、白黒反転する
            utils::cvt_color_to(stock_number_area_image, &mut work_capture_image, ColorFormat::GRAY as i32)?;
            core::bitwise_not(&work_capture_image, stock_number_area_image, &core::no_array())?;

            // 近似白黒処理して
            imgproc::threshold(stock_number_area_image, &mut work_capture_image, 200.0, 255.0, imgproc::THRESH_BINARY)?;
            utils::cvt_color_to(&work_capture_image, stock_number_area_image, ColorFormat::RGB as i32)?;

            // tesseract で文字(数値)を取得して, 余計な文字を排除
            let text = &async_std::task::block_on(utils::run_ocr_with_number(&stock_number_area_image, Some("0123"), true)).unwrap();
            let number = re.split(text).collect::<Vec<&str>>().join("").parse().unwrap_or(0);

            Ok(number)
        };
        
        let index_by_player_max = smashbros_data.get_player_count()/2-1;
        let mut ko_stock = Vec::new();
        let mut kd_stock = Vec::new();
        for player_number in 0..smashbros_data.get_player_count() {
            let mut stocks = [0; 3];
            for i in 0..=2 {
                let stock_number_pos = &Self::STOCK_AREA_POS[index_by_player_max as usize][player_number as usize][i];
                let mut stock_number_area_image = core::Mat::roi(&capture_image.clone(),
                    core::Rect{x:stock_number_pos.x + 8, y:stock_number_pos.y - 2, width:20 - 8, height:15 + 4})?;
                stocks[i] = capture_stock(&mut stock_number_area_image).unwrap_or(0);
            }

            let (p_ko_stock, fall_stock, sd_stock) = (stocks[0], -stocks[1].abs(), stocks[2]);
            ko_stock.push(p_ko_stock);
            kd_stock.push(-fall_stock + sd_stock);

            let number = smashbros_data.get_max_stock(player_number) + fall_stock - sd_stock;
            smashbros_data.guess_stock( player_number, number );
        }

        match smashbros_data.get_player_count() {
            2 => {
                if ko_stock[0] == kd_stock[1] && ko_stock[1] == kd_stock[0] {
                    // [撃墜, 落下] が自他ともに取れるとかなり確実な情報として処理
                    log::info!("stock: KOs:{:?} KDs:{:?}", ko_stock, kd_stock);
                    smashbros_data.set_stock(0, smashbros_data.get_max_stock(0) - kd_stock[0]);
                    smashbros_data.set_stock(1, smashbros_data.get_max_stock(1) - kd_stock[1]);
                }
            },
            _ => (),
        }

        Ok(smashbros_data.all_decided_stock())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_result_stock() {
        let mut data = SmashbrosData::default();
        data.initialize_battle(2, true);
        data.guess_character_name(0, "MARIO".to_string());
        data.set_max_stock(0, 3);
        data.set_max_stock(1, 3);

        let mut result_scene = ResultScene::default();
        ResultScene::capture_result_stock(
            &imgcodecs::imread("test/resource/result_stock.png", imgcodecs::IMREAD_COLOR).unwrap(),
            &mut data,
            &mut result_scene.result_stock_color
        ).unwrap();

        assert_eq!(data.get_stock(0), 1);
        assert_eq!(data.get_stock(1), 0);
    }
}

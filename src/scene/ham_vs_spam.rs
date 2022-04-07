use super::*;

/// キャラクターが大きく表示されてる画面
/// save: キャラクター名, ルール名, 取れるなら[時間,ストック,HP]
pub struct HamVsSpamScene {
    vs_scene_judgment: SceneJudgment,
    rule_stock_scene_judgment: SceneJudgment,
    rule_time_scene_judgment: SceneJudgment,
    rule_stamina_scene_judgment: SceneJudgment,
    buffer: CaptureFrameStore,
}
impl Default for HamVsSpamScene {
    fn default() -> Self {
        Self {
            vs_scene_judgment: SceneJudgment::new_with_lang("vs"),
            rule_stock_scene_judgment: SceneJudgment::new(
                    imgcodecs::imread("resource/rule_stock_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/rule_stock_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.985),
            rule_time_scene_judgment: SceneJudgment::new(
                    imgcodecs::imread("resource/rule_time_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/rule_time_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.985),
            rule_stamina_scene_judgment: SceneJudgment::new(
                    imgcodecs::imread("resource/rule_hp_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap(),
                    Some(imgcodecs::imread("resource/rule_hp_mask.png", imgcodecs::IMREAD_UNCHANGED).unwrap())
                ).unwrap()
                .set_border(0.985),
            buffer: CaptureFrameStore::default(),
        }
    }
}
impl SceneTrait for HamVsSpamScene {
    fn change_language(&mut self) { *self = Self::default(); }
    fn get_id(&self) -> i32 { SceneList::HamVsSpam as i32 }
    fn get_prev_match(&self) -> Option<&SceneJudgment> { Some(&self.vs_scene_judgment) }
    
    fn continue_match(&self, now_scene: SceneList) -> bool {
        match now_scene {
            SceneList::Matching => true,
            _ => false,
        }
    }

    fn is_scene(&mut self, capture_image: &core::Mat, smashbros_data: Option<&mut SmashbrosData>) -> opencv::Result<bool> {
        if let Some(smashbros_data) = smashbros_data.as_ref() {
            if smashbros_data.all_decided_character_name() {
                // すべてのプレイヤーが確定している場合は判定すら行わない (matchTemaplte は処理コストが高い)
                return Ok(false);
            }
        }

        async_std::task::block_on(async {
            self.vs_scene_judgment.match_captured_scene(&capture_image).await
        })?;

        if self.vs_scene_judgment.is_near_match() {
            imgcodecs::imwrite("ham_vs_spam.png", capture_image, &core::Vector::from(vec![]))?;
            self.buffer.start_recoding_by_time(std::time::Duration::from_millis(2500));
            self.buffer.recoding_frame(capture_image)?;
        }
        Ok(self.vs_scene_judgment.is_near_match())
    }

    fn to_scene(&self, _now_scene: SceneList) -> SceneList { SceneList::HamVsSpam }

    fn recoding_scene(&mut self, capture_image: &core::Mat) -> opencv::Result<()> { self.buffer.recoding_frame(capture_image) }
    fn is_recoded(&self) -> bool { self.buffer.is_filled() }

    /// save: キャラクターの種類, ルール(time | stock | stamina), 時間
    fn detect_data(&mut self, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let Self {
            rule_stock_scene_judgment,
            rule_time_scene_judgment,
            rule_stamina_scene_judgment,
            buffer,
            ..
        } = self;

        buffer.replay_frame(|frame| {
            Self::captured_rules(&frame, smashbros_data, rule_stock_scene_judgment, rule_time_scene_judgment, rule_stamina_scene_judgment)?;
            Self::captured_character_name(&frame, smashbros_data)?;

            Ok(false)
        })?;

        Ok(())
    }
}
impl HamVsSpamScene {
    pub fn captured_character_name(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<bool> {
        if smashbros_data.all_decided_character_name() {
            return Ok(true);
        }

        use regex::Regex;
        let mut gray_capture_image = core::Mat::default();
        utils::cvt_color_to(capture_image, &mut gray_capture_image, ColorFormat::GRAY as i32)?;

        // 近似白黒処理して
        let mut temp_capture_image = core::Mat::default();
        let mut work_capture_image = core::Mat::default();
        imgproc::threshold(&gray_capture_image, &mut work_capture_image, 200.0, 255.0, imgproc::THRESH_BINARY)?;
        core::bitwise_not(&work_capture_image, &mut temp_capture_image, &core::no_array())?;

        // プレイヤー毎の位置で処理する
        let (width, height) = (capture_image.cols(), capture_image.rows());
        let player_area_width = width / smashbros_data.get_player_count();
        let re = Regex::new(r"\s*(\w+)\s*").unwrap();
        let mut skip_count = 0;
        for player_count in 0..smashbros_data.get_player_count() {
            if smashbros_data.is_decided_character_name(player_count) {
                // 既にプレイヤーキャラクターが確定しているならスキップ
                skip_count += 1;
                continue;
            }
            // 高さそんなにいらないので適当に小さくする
            let player_name_area = core::Rect {
                x: player_area_width*player_count +30, y: 0,        // 30:{N}P のプレイヤー表示の幅
                width: player_area_width -20 -30, height: height/7  // 10:稲妻が処理後に黒四角形になって文字領域として誤認されるのを防ぐため
            };
            let mut name_area_image = core::Mat::roi(&temp_capture_image, player_name_area)?;
            let gray_name_area_image = core::Mat::roi(&work_capture_image, player_name_area)?;

            // 輪郭捕捉して
            let name_contour_image = utils::trimming_any_rect(
                &mut name_area_image, &gray_name_area_image, Some(5), None, None, false, None)?;
            utils::cvt_color_to(&name_contour_image, &mut name_area_image, ColorFormat::RGB as i32)?;

            // tesseract でキャラ名取得して, 余計な文字を排除
            let text = &async_std::task::block_on(utils::run_ocr_with_upper_alpha(&name_area_image)).unwrap();
            if let Some(caps) = re.captures( text ) {
                smashbros_data.guess_character_name( player_count, String::from(&caps[1]) );
            }
        }

        Ok(smashbros_data.get_player_count() == skip_count)
    }

    pub fn captured_rules(capture_image: &core::Mat, smashbros_data: &mut SmashbrosData, rule_stock_scene_judgment: &mut SceneJudgment, rule_time_scene_judgment: &mut SceneJudgment, rule_stamina_scene_judgment: &mut SceneJudgment) -> opencv::Result<bool> {
        if smashbros_data.get_rule() == BattleRule::Tournament {
            return Ok(false);
        }

        if !smashbros_data.is_decided_rule() {
            async_std::task::block_on(async {
                // ストック制と検出(1on1: これがデフォルトで一番多いルール)
                rule_stock_scene_judgment.match_captured_scene(capture_image).await?;
                if rule_stock_scene_judgment.is_near_match() {
                    smashbros_data.set_rule(BattleRule::Stock);
                    log::info!("rule: stock: {:2.3}%", rule_stock_scene_judgment.prev_match_ratio);
                    return Ok(());
                }

                rule_time_scene_judgment.match_captured_scene(capture_image).await?;
                if rule_time_scene_judgment.is_near_match() {
                    smashbros_data.set_rule(BattleRule::Time);
                    log::info!("rule: time {:2.3}%", rule_time_scene_judgment.prev_match_ratio);
                    return Ok(());
                }

                rule_stamina_scene_judgment.match_captured_scene(capture_image).await
            })?;
            if rule_stamina_scene_judgment.is_near_match() {
                smashbros_data.set_rule(BattleRule::Stamina);
                log::info!("rule: stamina {:2.3}%", rule_stamina_scene_judgment.prev_match_ratio);
            }
        }

        // 各ルール条件の検出
        let mut time_area: Option<core::Mat> = None;
        let mut sec_time_area: Option<core::Mat> = None;
        let mut stock_area: Option<core::Mat> = None;
        let mut hp_area: Option<core::Mat> = None;
        match smashbros_data.get_rule() {
            BattleRule::Time => {
                // Time   : 時間制限あり[2,2:30,3], ストック数は上限なしの昇順, HPはバースト毎に0%に初期化
                time_area = Some(core::Mat::roi( capture_image, core::Rect {x:313, y:332, width:10, height:20})? );
                sec_time_area = Some(core::Mat::roi( capture_image, core::Rect {x:325, y:332, width:18, height:20})? );
            },
            BattleRule::Stock => {
                // Stock  : 時間制限あり[3,4,5,6,7], ストック数は上限[1,2,3]の降順, HPはバースト毎に0%に初期化
                time_area = Some(core::Mat::roi( capture_image, core::Rect {x:274, y:332, width:11, height:20})? );
                stock_area = Some(core::Mat::roi( capture_image, core::Rect {x:358, y:332, width:11, height:20})? );
            },
            BattleRule::Stamina => {
                // Stamina: 時間制限あり[3,4,5,6,7], ストック数は上限[1,2,3]の降順, HPは上限[100,150,200,250,300]の降順
                time_area = Some(core::Mat::roi( capture_image, core::Rect {x:241, y:332, width:11, height:20})? );
                stock_area = Some(core::Mat::roi( capture_image, core::Rect {x:324, y:332, width:11, height:20})? );
                hp_area = Some(core::Mat::roi( capture_image, core::Rect {x:380, y:332, width:18, height:20})? );
            },
            _ => ()
        }

        if let Some(mut sec_time_area) = sec_time_area {
            Self::captured_time_with_sec(&mut time_area.unwrap(), &mut sec_time_area, smashbros_data)?;
        } else if let Some(mut time_area) = time_area {
            Self::captured_time(&mut time_area, smashbros_data)?;
        }
        if let Some(mut stock_area) = stock_area {
            Self::captured_stock(&mut stock_area, smashbros_data)?;
        }
        if let Some(mut hp_area) = hp_area {
            Self::captured_stamina(&mut hp_area, smashbros_data)?;
        }

        // ストック と 制限時間 が下から上がってくる演出を出していて、誤検出しやすいので, frame を全部処理する
        Ok(smashbros_data.is_decided_rule_all_clause())
    }

    pub fn captured_time(capture_image: &mut core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let time = match Self::captured_convert_number(capture_image, r"\s*(\d)\s*", Some("34567"), true) {
            Ok(time) => time.parse::<u64>().unwrap_or(0) * 60,
            Err(_) => 0,
        };

        smashbros_data.guess_max_time(time);

        Ok(())
    }

    pub fn captured_time_with_sec(capture_image: &mut core::Mat, sec_capture_image: &mut core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let time = match Self::captured_convert_number(capture_image, r"\s*(\d)\s*", Some("23"), true) {
            Ok(time) => time.parse::<u64>().unwrap_or(0) * 60,
            Err(_) => 0,
        };
        let sec_time = match Self::captured_convert_number(sec_capture_image, r"\s*(\d+)\s*", Some("03"), false) {
            Ok(sec_time) => sec_time.parse::<u64>().unwrap_or(0),
            Err(_) => 0,
        };

        smashbros_data.guess_max_time(time + sec_time);

        Ok(())
    }

    pub fn captured_stock(capture_image: &mut core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let stock: i32 = match Self::captured_convert_number(capture_image, r"\s*(\d)\s*", Some("123"), true) {
            Ok(stock) => stock.parse().unwrap_or(-1),
            Err(_) => 0,
        };

        for player_number in 0..smashbros_data.get_player_count() {
            smashbros_data.guess_max_stock(player_number, stock);
        }

        Ok(())
    }

    pub fn captured_stamina(capture_image: &mut core::Mat, smashbros_data: &mut SmashbrosData) -> opencv::Result<()> {
        let hp = match Self::captured_convert_number(capture_image, r"\s*(\d+)\s*", Some("01235"), false) {
            Ok(hp) => hp.parse().unwrap_or(-1) * 10,
            Err(_) => 0,
        };

        for player_number in 0..smashbros_data.get_player_count() {
            smashbros_data.guess_max_hp(player_number, hp);
        }


        Ok(())
    }

    /// capture_image から検出した文字列を regex_pattern で正規表現にかけて文字列(数値)にして返す
    pub fn captured_convert_number(capture_image: &mut core::Mat, regex_pattern: &str, valid_string: Option<&str>, is_single_char: bool) -> opencv::Result<String> {
        use regex::Regex;
        // 近似白黒処理して
        let mut gray_capture_image = core::Mat::default();
        imgproc::threshold(capture_image, &mut gray_capture_image, 100.0, 255.0, imgproc::THRESH_BINARY)?;
        let mut work_capture_image = core::Mat::default();
        utils::cvt_color_to(&gray_capture_image, &mut work_capture_image, ColorFormat::GRAY as i32)?;

        // 白黒反転して
        core::bitwise_not(&work_capture_image, &mut gray_capture_image, &core::no_array())?;

        // tesseract で文字(数値)を取得して, 余計な文字を排除
        let text = &async_std::task::block_on(utils::run_ocr_with_number(&gray_capture_image, valid_string, is_single_char)).unwrap().to_string();
        let re = Regex::new(regex_pattern).unwrap();
        if let Some(caps) = re.captures( text ) {
            return Ok( caps[1].to_string() );
        }

        Err(opencv::Error::new( 0, "not found anything. from captured_convert_number".to_string() ))
    }
}

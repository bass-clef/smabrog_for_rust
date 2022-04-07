use super::*;

/// シーン判定汎用クラス
pub struct SceneJudgment {
    color_image: core::Mat,
    mask_image: Option<core::Mat>,
    trans_mask_image: Option<core::Mat>,
    image_size: Option<core::Rect>,
    judgment_type: ColorFormat,
    pub prev_match_ratio: f64,
    pub prev_match_point: core::Point,
    border_match_ratio: f64,
}
impl Default for SceneJudgment {
    fn default() -> Self {
        Self {
            color_image: core::Mat::default(),
            mask_image: None,
            trans_mask_image: None,
            image_size: None,
            judgment_type: ColorFormat::RGB,
            border_match_ratio: 0.98,
            prev_match_ratio: 0f64,
            prev_match_point: Default::default(),
        }
    }
}
impl SceneJudgment {
    // 言語によって読み込むファイルを変えて作成する
    pub fn news_with_lang<T>(new_func: T, name: &str) -> Self
    where T: Fn(core::Mat, Option<core::Mat>) -> opencv::Result<Self>
    {
        use crate::resource::LANG_LOADER;
        use i18n_embed::LanguageLoader;

        let lang = LANG_LOADER().get().current_language().language.clone();
        let path = format!("resource/{}_{}", lang.as_str(), name);

        new_func(
            imgcodecs::imread(&format!("{}_color.png", path), imgcodecs::IMREAD_UNCHANGED).unwrap(),
            Some(imgcodecs::imread(&format!("{}_mask.png", path), imgcodecs::IMREAD_UNCHANGED).unwrap())
        ).unwrap()
    }

    pub fn new_gray_with_lang(name: &str) -> Self { Self::news_with_lang(Self::new_gray, name) }
    pub fn new_with_lang(name: &str) -> Self { Self::news_with_lang(Self::new, name) }
    // fn new_trans_with_lang(name: &str) -> Self { Self::news_with_lang(Self::new_trans, name) }

    /// color_format に {hoge}_image を強制して、一致させるシーン
    pub fn new_color_format(color_image: core::Mat, mask_image: Option<core::Mat>, color_format: ColorFormat) -> opencv::Result<Self> {
        let mut converted_color_image = core::Mat::default();

        // 強制で color_format にする
        utils::cvt_color_to(&color_image, &mut converted_color_image, color_format as i32)?;

        let converted_mask_image = match &mask_image {
            Some(v) => {
                let mut converted_mask_image = core::Mat::default();
                utils::cvt_color_to(&v, &mut converted_mask_image, color_format as i32)?;

                Some(converted_mask_image)
            },
            None => None,
        };

        Ok(Self {
            color_image: converted_color_image, mask_image: converted_mask_image,
            trans_mask_image: None,
            judgment_type: color_format,
            ..Default::default()
        })
    }
    /// 白黒画像と一致するシーン
    pub fn new_gray(color_image: core::Mat, mask_image: Option<core::Mat>) -> opencv::Result<Self> {
        Self::new_color_format(color_image, mask_image, ColorFormat::GRAY)
    }
    /// 普通のRGB画像と一致するシーン
    pub fn new(color_image: core::Mat, mask_image: Option<core::Mat>) -> opencv::Result<Self> {
        Self::new_color_format(color_image, mask_image, ColorFormat::RGB)
    }
    /// 透過画像と一致するシーン
    pub fn new_trans(color_image: core::Mat, mask_image: Option<core::Mat>) -> opencv::Result<Self> {
        // 他のメンバを透過に変換しておく
        let mut converted_color_image = core::Mat::default();
        utils::cvt_color_to(&color_image, &mut converted_color_image, ColorFormat::RGBA as i32)?;

        // 透過画像の場合は予め透過マスクを作成する
        let mut converted_mask_image = core::Mat::default();
        let mut trans_mask_image = core::Mat::default();
        match &mask_image {
            Some(v) => {
                // 強制で RGBA にする
                utils::cvt_color_to(&v, &mut trans_mask_image, ColorFormat::RGBA as i32)?;
                utils::cvt_color_to(&v, &mut converted_mask_image, ColorFormat::RGBA as i32)?;
            },
            None => {
                utils::make_trans_mask_from_noalpha(&color_image, &mut trans_mask_image)?;
            },
        }

        Ok(Self {
            color_image: converted_color_image, mask_image: Some(converted_mask_image),
            trans_mask_image: Some(trans_mask_image),
            judgment_type: ColorFormat::RGBA,
            ..Default::default()
        })
    }

    /// 一致率の上限の設定 [default = 0.99]
    pub fn set_border(mut self, border_match_ratio: f64) -> Self {
        self.border_match_ratio = border_match_ratio;

        self
    }
    
    /// 検出領域の設定 [default = full]
    pub fn set_size(mut self, new_size: core::Rect) -> Self {
        self.image_size = Some(new_size);

        self.color_image = core::Mat::roi(&self.color_image.clone(), new_size).unwrap();

        if let Some(trans_mask_image) = self.trans_mask_image {
            self.trans_mask_image = Some(core::Mat::roi(&trans_mask_image, new_size).unwrap());
        }
        if let Some(mask_image) = self.mask_image {
            self.mask_image = Some(core::Mat::roi(&mask_image, new_size).unwrap());
        }

        self
    }

    /// キャプチャされた画像とシーンとをテンプレートマッチングして、一致した確率と位置を返す
    pub async fn match_captured_scene(&mut self, captured_image: &core::Mat) -> opencv::Result<()> {
        let mut result = core::Mat::default();
        let mut converted_captured_image = core::Mat::default();
        if let Some(image_size) = self.image_size {
            utils::cvt_color_to(
                &core::Mat::roi(&captured_image, image_size)?,
                &mut converted_captured_image, self.judgment_type as i32
            )?;
        } else {
            utils::cvt_color_to(captured_image, &mut converted_captured_image, self.judgment_type as i32)?;
        }
        
        match self.judgment_type {
            ColorFormat::NONE => (),
            ColorFormat::RGB | ColorFormat::GRAY => {
                // [2値 | RGB]画像はマスクがあれば and かけて、ないならテンプレートマッチング
                // None の場合は converted_captured_image はコピーされた状態だけでよい
                if let Some(mask_image) = &self.mask_image {
                    // captured_image を mask_image で篩いにかけて,無駄な部分を削ぐ
                    // どうでもいいけどソースをみてそれに上書きしてほしいとき、同じ変数を指定できないの欠陥すぎね？？？(これが安全なメモリ管理か、、、。)
                    let mut temp_captured_image = converted_captured_image.clone();

                    // サイズの違いや色深度の違いでエラーになることがあるけど、マスクがかけられないこともある
                    core::bitwise_and(&converted_captured_image, &mask_image,
                        &mut temp_captured_image, &core::no_array())?;

                    converted_captured_image = temp_captured_image;
                }

                imgproc::match_template(&converted_captured_image, &self.color_image, &mut result,
                    imgproc::TM_CCOEFF_NORMED, &core::no_array())?;
            },
            ColorFormat::RGBA => {
                // 透過画像の場合は普通に trans_mask 付きでテンプレートマッチング
                // 透過画像の時はそもそも None の状態になることはない
                if let Some(trans_mask_image) = &self.trans_mask_image {
                    imgproc::match_template(&converted_captured_image, &self.color_image, &mut result,
                        imgproc::TM_CCORR_NORMED, &trans_mask_image)?;
                }
            },
        };

        core::patch_na_ns(&mut result, -0.0)?;
        utils::patch_inf_ns(&mut result, -0.0)?;

        core::min_max_loc(&result,
            None, Some(&mut self.prev_match_ratio),
            None, Some(&mut self.prev_match_point),
            &core::no_array()
        )?;

        Ok(())
    }

    /// 前回のテンプレートマッチングで大体一致しているか
    pub fn is_near_match(&self) -> bool {
        self.border_match_ratio <= self.prev_match_ratio
    }

    /// is_near_match が確定するのに必要な確率
    pub fn get_border_match_ratio(&self) -> f64 {
        self.border_match_ratio
    }
}

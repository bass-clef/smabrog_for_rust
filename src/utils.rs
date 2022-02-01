
pub mod utils {
    use opencv::{
        core,
        imgproc,
        prelude::*
    };
    use tesseract::Tesseract;
    
    // if-else も match もさけようね～というやつ
    const COLOR_MAP: [[i32; 5]; 5] = [
        [-1, -1, -1, -1, -1],    // null to null
        [-1, -1, -1, imgproc::COLOR_GRAY2RGB, imgproc::COLOR_GRAY2RGBA],   // gray to hoge
        [-1, -1, -1, -1, -1],    // unknown to unknown
        [-1, imgproc::COLOR_RGB2GRAY, -1, -1, imgproc::COLOR_RGB2RGBA],   // rgb to hoge
        [-1, imgproc::COLOR_RGBA2GRAY, -1, imgproc::COLOR_RGBA2RGB, 0],   // rgba to hoge
    ];

    /// src.channels() に応じて to_channels に変換する cvt_color をする
    /// src.channels() == to_channels なら src.copy_to(dst) をする
    pub fn cvt_color_to(src: &core::Mat, dst: &mut core::Mat, to_channels: i32) -> opencv::Result<()> {
        let color_map = COLOR_MAP[src.channels() as usize][to_channels as usize];
        if -1 == color_map {
            // コピーだけする
            src.copy_to(dst)?;
            return Ok(());
        }

        imgproc::cvt_color(src, dst, color_map, 0)?;
        Ok(())
    }

    /// OpenCV に処理するメソッドがないため定義。(NaN はあるのにどうして inf は無いんだ？？？)
    pub fn patch_inf_ns(data: &mut core::Mat, to_value: f32) -> opencv::Result<()> {
        for y in 0..data.cols() {
            for x in 0..data.rows() {
                let value = data.at_mut::<f32>(y * data.rows() + x)?;
                if *value == std::f32::INFINITY || *value == std::f32::NEG_INFINITY {
                    *value = to_value;
                }
            }
        }
        Ok(())
    }

    /// src に対しての特定色を透過色とした mask を作成
    pub fn make_trans_mask_from_noalpha(src: &core::Mat, dst: &mut core::Mat) -> opencv::Result<()> {
        let trans_color = [0.0, 0.0, 0.0, 1.0];
        let lower_mat = core::Mat::from_slice(&trans_color)?;
        let upper_mat = core::Mat::from_slice(&trans_color)?;
        let mut mask = core::Mat::default();
        core::in_range(&src, &lower_mat, &upper_mat, &mut mask)?;
        core::bitwise_not(&mask, dst, &core::no_array())?;

        Ok(())
    }

    /// 任意の四角形の中にある何かの輪郭にそって src を加工して返す
    pub fn trimming_any_rect(src: &mut core::Mat, gray_src: &core::Mat, margin: Option<i32>,
        min_size: Option<f64>, max_size: Option<f64>, noise_fill: bool, noise_color: Option<core::Scalar>)
    -> opencv::Result<core::Mat>
    {
        let mut contours = opencv::types::VectorOfVectorOfPoint::new();
        let (width, height) = (src.cols(), src.rows());
        let mut any_rect = core::Rect::new(width, height, 0, 0);
        imgproc::find_contours(gray_src, &mut contours, imgproc::RETR_EXTERNAL, imgproc::CHAIN_APPROX_SIMPLE, core::Point{x:0,y:0})?;

        for (i, contour) in &mut contours.to_vec().iter_mut().enumerate() {
            let area_contours = opencv::types::VectorOfPoint::from_iter(contour.iter());
            let area = imgproc::contour_area(&area_contours, false)?;
            // ノイズの除去 or スキップ
            if area < min_size.unwrap_or(10.0) {
                if noise_fill {
                    imgproc::draw_contours(
                        src, &contours, i as i32, noise_color.unwrap_or(core::Scalar::new(255.0, 255.0, 255.0, 0.0)),
                        1, imgproc::LINE_8, &core::no_array(), std::i32::MAX, core::Point{x:0,y:0})?;
                }
                continue;
            } else if max_size.unwrap_or(10_000.0) < area {
                if noise_fill && max_size.is_some() {
                    imgproc::draw_contours(
                        src, &contours, i as i32, noise_color.unwrap_or(core::Scalar::new(255.0, 255.0, 255.0, 0.0)),
                        1, imgproc::LINE_8, &core::no_array(), std::i32::MAX, core::Point{x:0,y:0})?;
                }
                continue;
            }

            let rect = imgproc::bounding_rect(&area_contours)?;
            any_rect.x = std::cmp::min(any_rect.x, rect.x);
            any_rect.y = std::cmp::min(any_rect.y, rect.y);
            any_rect.width = std::cmp::max(any_rect.width, rect.x + rect.width);
            any_rect.height = std::cmp::max(any_rect.height, rect.y + rect.height);
        }

        let mut trimming_rect = core::Rect {
            x: std::cmp::max(any_rect.x - margin.unwrap_or(0), 0),
            y: std::cmp::max(any_rect.y - margin.unwrap_or(0), 0),
            width: std::cmp::min(any_rect.width + margin.unwrap_or(0), width),
            height: std::cmp::min(any_rect.height + margin.unwrap_or(0), height)};
        trimming_rect.width -= trimming_rect.x + 1;
        trimming_rect.height -= trimming_rect.y + 1;

        match core::Mat::roi(&src, trimming_rect) {
            Ok(result_image) => Ok(result_image),
            // size が 0 近似で作成できないときが予想されるので、src を返す
            Err(_) => Ok(src.clone()),
        }
    }

    /// Tesseract-OCR を Mat で叩く
    /// tesseract::ocr_from_frame だと「Warning: Invalid resolution 0 dpi. Using 70 instead.」がうるさかったので作成
    pub fn ocr_with_mat(image: &core::Mat) -> Tesseract {
        let size = image.channels() * image.cols() * image.rows();
        let data: &[u8] = unsafe{ std::slice::from_raw_parts(image.datastart(), size as usize) };

        match Tesseract::new(None, Some("eng")) {
            Ok(tess) => {
                tess.set_page_seg_mode(tesseract_sys::TessPageSegMode_PSM_SINGLE_BLOCK)
                    .set_frame(data, image.cols(), image.rows(),
                        image.channels(), image.channels() * image.cols()).unwrap()
                    .set_source_resolution(70)
            },
            Err(err) => {
                log::error!("{}", err);
                panic!("{}\nFailed use tesseract. please reinstall smabrog.", err);
            },
        }
    }
    /// OCR(大文字アルファベットのみを検出)
    pub async fn run_ocr_with_upper_alpha(image: &core::Mat) -> Result<String, tesseract::TesseractError> {
        Ok(
            ocr_with_mat(image)
                .set_variable("tessedit_char_whitelist", "ABCDEFGHIJKLMNOPQRSTUVWXYZ").unwrap()
                .recognize()?
                .get_text().unwrap_or("".to_string())
                .replace("\n", "")
        )
    }
    /// OCR(数値を検出)
    pub async fn run_ocr_with_number(image: &core::Mat) -> Result<String, tesseract::TesseractError> {
        Ok(
            ocr_with_mat(image)
                .set_variable("tessedit_char_whitelist", "0123456789-.").unwrap()
                .recognize()?
                .get_text().unwrap_or("".to_string())
        )
    }

    /// &str -> WCHAR
    pub fn to_wchar(value: &str) -> *mut winapi::ctypes::wchar_t {
        use std::os::windows::ffi::OsStrExt;

        let mut vec16 :Vec<u16> = std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        vec16.as_mut_ptr()
    }
}

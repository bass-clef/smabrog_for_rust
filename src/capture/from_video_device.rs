use super::*;

/// ビデオキャプチャ から Mat
pub struct CaptureFromVideoDevice {
    pub video_capture: Box<dyn videoio::VideoCaptureTrait>,
    pub prev_image: core::Mat,
    pub empty_data: core::Mat,
}
impl CaptureTrait for CaptureFromVideoDevice {
    fn get_mat(&mut self) -> anyhow::Result<core::Mat> {
        // video_capture が開かれていない(録画されていない) or 準備が整ってない
        if !self.video_capture.is_opened()? || !self.video_capture.grab()? {
            self.prev_image = self.empty_data.clone();
            return Ok(self.prev_image.try_clone()?);
        }

        // 1 frame 取得
        if !self.video_capture.retrieve(&mut self.prev_image, 0)? {
            // Video Device は不安定で取得できない場合が多々あるっぽいので、予め用意した空の Mat を返す
            self.prev_image = self.empty_data.clone();
        }

        Ok(self.prev_image.try_clone()?)
    }
}
impl CaptureFromVideoDevice {
    pub fn new(index: i32) -> anyhow::Result<Self> {
        let mut video_capture = match videoio::VideoCapture::new(index, videoio::CAP_DSHOW) {
            Ok(video_capture) => {
                log::info!("capture video device {}.", index);
                Box::new(video_capture)
            },
            Err(e) => {
                log::error!("{}", e);
                anyhow::bail!("{}", e)
            },
        };
        video_capture.set(opencv::videoio::CAP_PROP_FRAME_WIDTH, 640f64)?;
        video_capture.set(opencv::videoio::CAP_PROP_FRAME_HEIGHT, 360f64)?;

        let mut own = Self {
            video_capture,
            prev_image: core::Mat::default(),
            empty_data: unsafe{ core::Mat::new_rows_cols(360, 640, core::CV_8UC3)? },
        };

        // 1回目テスト
        let mut ready_to_fight_scene = ReadyToFightScene::default();
        let capture_image = match own.get_mat() {
            Ok(capture_image) => capture_image,
            Err(e) => {
                log::error!("{}", e);
                anyhow::bail!("{}", e)
            },
        };

        ready_to_fight_scene.is_scene(&capture_image, None).unwrap();
        let scene_judgment = ready_to_fight_scene.get_prev_match().unwrap();
        if !scene_judgment.is_near_match() {
            anyhow::bail!("not capture ReadyToFight. max ratio: {} < {}",
                scene_judgment.prev_match_ratio,
                scene_judgment.get_border_match_ratio()
            );
        }

        log::info!("match device:{:3.3}% id:{}", scene_judgment.prev_match_ratio, index);

        Ok(own)
    }

    /// OpenCV で VideoCapture の ID の一覧が取れないので(永遠の謎)、取得して返す
    /// Rust での DirectX系のAPI, COM系が全然わからんかったので、
    /// C++ の MSDN のソースを python でコンパイルした exe を実行して正規表現にかけて返す
    /// @return Option<Vec<(i32, String)>> 無いか、[DeviceName]と[DeviceID]が入った[HashMap]
    pub fn get_device_list() -> Vec<String> {
        let mut device_list: Vec<String> = Vec::new();

        if let Ok(output) = std::process::Command::new("video_device_list.exe").output() {
            let re = regex::Regex::new(r"(?P<id>\d+):(?P<name>.+)\r\n").unwrap();
            for caps in re.captures_iter( std::str::from_utf8(&output.stdout).unwrap_or("") ) {
                device_list.push( String::from(&caps["name"]) );
            }
        } else {
            log::error!("output is none by video_device_list.exe");
        }

        device_list
    }
}

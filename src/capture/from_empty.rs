use super::*;

/// デフォルト用の空 Mat
pub struct CaptureFromEmpty {
    pub prev_image: core::Mat,
}
impl CaptureTrait for CaptureFromEmpty {
    fn get_mat(&mut self) -> anyhow::Result<core::Mat> { Ok(self.prev_image.try_clone()?) }
}
impl CaptureFromEmpty {
    pub fn new() -> opencv::Result<Self> {
        Ok(Self {
            prev_image: imgcodecs::imread("resource/loading_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap()
        })
    }
}

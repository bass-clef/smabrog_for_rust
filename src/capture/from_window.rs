use dxcapture::{
    enumerate_windows,
    Capture,
    Device,
};
use super::*;

/// ウィンドウ から Mat
pub struct CaptureFromWindow {
    pub base: CaptureBase,
    _device: Device,
    capture: Capture,
}
impl CaptureTrait for CaptureFromWindow {
    fn get_mat(&mut self) -> anyhow::Result<core::Mat> {
        let mat = self.capture.get_mat_frame()?;

        Ok(self.base.get_mat(mat.data)?)
    }
}
impl CaptureFromWindow {
    pub fn new(win_caption: &str) -> anyhow::Result<Self> {
        let device = Device::new_from_window(win_caption.to_string())?;
        let capture = Capture::new(&device)?;
        let mat = capture.wait_mat_frame()?;
        let base = CaptureBase::new_from_some_types_mat(mat.data)?;

        Ok(Self {
            base,
            _device: device,
            capture,
        })
    }

    /// ウィンドウ名を列挙して返す
    pub fn get_window_list() -> Vec<String> {
        enumerate_windows().iter().map(|w| w.title.clone()).collect()
    }
}

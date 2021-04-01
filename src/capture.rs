#![cfg(windows)]
#![windows_subsystem = "windows"]

use opencv::{
    core,
    prelude::MatTrait,
    videoio,
};

use crate::gui::GUI;


/* &str -> WCHAR */
fn to_wchar(value: &str) -> *mut winapi::ctypes::wchar_t {
    use std::os::windows::ffi::OsStrExt;

    let mut vec16 :Vec<u16> = std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    vec16.as_mut_ptr()
}

/* 画面をキャプチャするクラス () */
pub trait Capture {
    /// Mat を返す
    fn get_mat(&mut self) -> opencv::Result<core::Mat>;
}

pub struct CaptureNone {
    pub prev_image: core::Mat,
}
impl Capture for CaptureNone {
    fn get_mat(&mut self) -> opencv::Result<core::Mat> { Ok(self.prev_image.try_clone()?) }
}
impl Default for CaptureNone {
    fn default() -> Self { Self {
        prev_image: core::Mat::default().unwrap()
    } }
}

// ビデオキャプチャ から Mat
pub struct CaptureFromVideoDevice {
    video_capture: Box<dyn videoio::VideoCaptureTrait>,
    pub prev_image: core::Mat,
}
impl Capture for CaptureFromVideoDevice {
    fn get_mat(&mut self) -> opencv::Result<core::Mat> {
        // Err の場合 Mat::default を返してほしい
        self.video_capture.grab()?;
        self.video_capture.retrieve(&mut self.prev_image, 0)?;

        Ok(self.prev_image.try_clone()?)
    }
}
impl Default for CaptureFromVideoDevice {
    fn default() -> Self {
        let mut own = CaptureFromVideoDevice::new(3);
        own.video_capture.set(opencv::videoio::CAP_PROP_FRAME_WIDTH, 640f64).unwrap();
        own.video_capture.set(opencv::videoio::CAP_PROP_FRAME_HEIGHT, 360f64).unwrap();
        own.video_capture.set(opencv::videoio::CAP_PROP_FPS, 30f64).unwrap();
        
        own
    }
}
impl CaptureFromVideoDevice {
    pub fn new(index: i32) -> Self {
        let mut active_device = vec![];
        let mut temp_video_capture: Box<dyn videoio::VideoCaptureTrait>;
        for i in 1..20 {
            match videoio::VideoCapture::new(i, videoio::CAP_ANY) {
                Ok(v) => {
                    // Rust ダウンキャストめんどくさくね？？？
                    temp_video_capture = Box::new(v);
                    if temp_video_capture.is_opened().unwrap() {
                        active_device.push(i);
                        println!("devs {}", i);
                    }
                    temp_video_capture.release().unwrap();
                },
                Err(_) => (),
            };

        }

        Self {
            video_capture: Box::new(videoio::VideoCapture::new(index, videoio::CAP_DSHOW).unwrap()),
            prev_image: core::Mat::default().unwrap(),
        }
    }
}


// ウィンドウ から Mat
// TODO:ウィンドウ の上位互換を実装できそうなのでいらないかも
pub struct CaptureFromWindow {
    pub win_caption: String,
    pub win_class: String,
    win_handle: winapi::shared::windef::HWND,
    pub prev_image: core::Mat,
}
impl Capture for CaptureFromWindow {
    fn get_mat(&mut self) -> opencv::Result<core::Mat> {
        self.update_dc_to_mat()?;

        Ok(self.prev_image.try_clone()?)
    }
}
impl Default for CaptureFromWindow {
    fn default() -> Self {
        CaptureFromWindow::new("", "")
    }
}
impl CaptureFromWindow {
    // ウィンドウに張り付いてる BMP を取得して Mat に変換する
    fn update_dc_to_mat(&mut self) -> opencv::Result<()> {
        use winapi::shared::minwindef::LPVOID;
        use winapi::shared::windef::HBITMAP;
        use winapi::um::wingdi::*;
        use winapi::um::winnt::HANDLE;

        if self.win_handle.is_null() {
            return Ok(());
        }

        unsafe {
            let dc_handle = winapi::um::winuser::GetDC(self.win_handle);
            let bitmap_handle: HBITMAP = GetCurrentObject(dc_handle, OBJ_BITMAP) as HBITMAP;
    
            let mut bitmap: BITMAP = std::mem::zeroed();
            GetObjectW(bitmap_handle as HANDLE, std::mem::size_of::<BITMAP>() as i32, &mut bitmap as PBITMAP as LPVOID);

            let color_number = bitmap.bmBitsPixel as i32 / 8;
            let size = (bitmap.bmWidth * bitmap.bmHeight * color_number) as usize;
    
            // TODO:Rust で LPVOID なメモリを確保する方法がわからない件について
            let mut buffer = Vec::<u8>::with_capacity(size);
            buffer.set_len(size);
            let pointer = Box::into_raw(buffer.into_boxed_slice());
            bitmap.bmBits = pointer as LPVOID;
            GetBitmapBits(bitmap_handle, size as i32, bitmap.bmBits);

            // オーバーヘッドやばいけど作成し直したほうが確実だった
            let temp_mat = core::Mat::new_rows_cols_with_data(
                bitmap.bmHeight, bitmap.bmWidth, core::CV_MAKETYPE(core::CV_8U, color_number),
                bitmap.bmBits, core::Mat_AUTO_STEP
            )?;

            // メモリ解放する前に clone する。
            // 上記関数はコピーでなくてメモリ自体を持ってるだけらしいので
            // 今後の mat アクセス系関数でメモリ系の例外ぶんなげられて、原因がわけわからなくなるのを防ぐ
            self.prev_image.release()?;
            self.prev_image = temp_mat.clone();

            // メモリ開放
            let s = std::slice::from_raw_parts_mut(bitmap.bmBits, size);
            let _ = Box::from_raw(s);
        }

        Ok(())
    }

    pub fn new(win_caption: &str, win_class: &str) -> Self {
        let win_handle = unsafe {
            winapi::um::winuser::FindWindowW(
                if win_class.is_empty() { std::ptr::null_mut() } else { to_wchar(win_class) },
                if win_caption.is_empty() { std::ptr::null_mut() } else { to_wchar(win_caption) }
            )
        };

        Self {
            win_caption: win_caption.to_string(),
            win_class: win_class.to_string(),
            win_handle: win_handle,
            prev_image: core::Mat::default().unwrap(),
        }
    }
}

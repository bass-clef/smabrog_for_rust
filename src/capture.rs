#![cfg(windows)]
#![windows_subsystem = "windows"]

use i18n_embed_fl::fl;
use opencv::{
    core,
    imgcodecs,
    imgproc,
    prelude::*,
    videoio,
};
use serde::Serialize;
use winapi::shared::minwindef::LPVOID;
use winapi::shared::windef::*;
use winapi::um::wingdi::*;
use winapi::um::winnt::HANDLE;
use winapi::um::winuser::*;

use crate::resource::LANG_LOADER;
use crate::scene::{
    ReadyToFightScene,
    SceneTrait,
};

pub mod codec;
pub mod frame_store;
pub mod retro;
pub mod base;
pub mod from_desktop;
pub mod from_empty;
pub mod from_video_device;
pub mod from_window;

pub use codec::*;
pub use frame_store::*;
pub use retro::*;
pub use base::CaptureBase;
pub use from_desktop::CaptureFromDesktop;
pub use from_empty::CaptureFromEmpty;
pub use from_video_device::CaptureFromVideoDevice;
pub use from_window::CaptureFromWindow;


// 検出する方法
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum CaptureMode {
    Empty(String),
    Desktop(String),
    /// _, device_id
    VideoDevice(String, i32, String),
    /// _, window_caption
    Window(String, String),
}
impl CaptureMode {
    pub fn new_empty() -> Self { Self::Empty { 0: fl!(LANG_LOADER().get(), "empty") } }
    pub fn new_desktop() -> Self { Self::Desktop { 0: fl!(LANG_LOADER().get(), "desktop") } }
    pub fn new_video_device(device_id: i32) -> Self {
        Self::VideoDevice { 0: fl!(LANG_LOADER().get(), "video_device"), 1:device_id, 2:String::new() }
    }
    pub fn new_window(win_caption: String) -> Self {
        Self::Window { 0: fl!(LANG_LOADER().get(), "window"), 1:win_caption }
    }

    pub fn is_default(&self) -> bool {
        if Self::new_empty() == *self {
            return true;
        }
        if Self::new_desktop() == *self {
            return true;
        }
        if Self::new_video_device(-1) == *self {
            return true;
        }
        if Self::new_window(String::new()) == *self {
            return true;
        }
        return false;
    }

    pub fn is_empty(&self) -> bool {
        if let Self::Empty(_) = self { true } else { false }
    }
    pub fn is_desktop(&self) -> bool {
        if let Self::Desktop(_) = self { true } else { false }
    }
    pub fn is_video_device(&self) -> bool {
        if let Self::VideoDevice(_, _, _) = self { true } else { false }
    }
    pub fn is_window(&self) -> bool {
        if let Self::Window(_, _) = self { true } else { false }
    }
}
impl Default for CaptureMode {
    fn default() -> Self {
        Self::new_empty()
    }
}
impl std::fmt::Display for CaptureMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Empty(show_text) | Self::Desktop(show_text)
                    | Self::VideoDevice(show_text, _, _) | Self::Window(show_text, _) => show_text
            }
        )
    }
}
impl AsMut<CaptureMode> for CaptureMode {
    fn as_mut(&mut self) -> &mut CaptureMode { self }
}
impl AsRef<CaptureMode> for CaptureMode {
    fn as_ref(&self) -> &CaptureMode { self }
}


/// Hoge をキャプチャするクラス
pub trait CaptureTrait {
    /// Mat を返す
    fn get_mat(&mut self) -> anyhow::Result<core::Mat>;
}

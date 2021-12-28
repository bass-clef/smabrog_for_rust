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

use crate::resource::lang_loader;
use crate::scene::{
    ReadyToFightScene,
    SceneTrait
};
use crate::utils::utils::to_wchar;


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
    pub fn new_empty() -> Self { Self::Empty { 0: fl!(lang_loader().get(), "empty") } }
    pub fn new_desktop() -> Self { Self::Desktop { 0: fl!(lang_loader().get(), "desktop") } }
    pub fn new_video_device(device_id: i32) -> Self {
        Self::VideoDevice { 0: fl!(lang_loader().get(), "video_device"), 1:device_id, 2:String::new() }
    }
    pub fn new_window(win_caption: String) -> Self {
        Self::Window { 0: fl!(lang_loader().get(), "window"), 1:win_caption }
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


/// Mat,DCを保持するクラス。毎回 Mat を作成するのはやはりメモリコストが高すぎた。
struct CaptureDC {
    prev_image: core::Mat,
    compatibility_dc_handle: HDC,
    pixel_buffer_pointer: LPVOID,
    bitmap: BITMAP,
    size: usize,
    width: i32, height: i32,
}
impl Default for CaptureDC {
    fn default() -> Self {
        Self {
            prev_image: core::Mat::default(),
            compatibility_dc_handle: 0 as HDC,
            pixel_buffer_pointer: 0 as LPVOID,
            bitmap: unsafe{ std::mem::zeroed() },
            size: 0,
            width: 0, height: 0,
        }
    }
}
impl Drop for CaptureDC {
    fn drop(&mut self) {
        self.release();
    }
}
impl CaptureDC {
    /// メモリ解放
    fn release(&mut self) {
        unsafe {
            if self.pixel_buffer_pointer.is_null() {
                return;
            }
            DeleteDC(self.compatibility_dc_handle);
            self.compatibility_dc_handle = 0 as HDC;

            let s = std::slice::from_raw_parts_mut(self.pixel_buffer_pointer, self.size);
            let _ = Box::from_raw(s);
            self.pixel_buffer_pointer = 0 as LPVOID;
        }
    }

    /// 状況に応じて handle から Mat を返す
    fn get_mat(&mut self, handle: winapi::shared::windef::HWND, content_area: Option<core::Rect>, offset_pos: Option<core::Point>) -> opencv::Result<core::Mat> {
        if self.compatibility_dc_handle.is_null() {
            self.get_mat_from_hwnd(handle, content_area, offset_pos)
        } else {
            self.get_mat_from_dc(handle, content_area, offset_pos)
        }
    }

    /// 既に作成してある互換 DC に HWND -> HDC から取得して,メモリコピーして Mat を返す
    fn get_mat_from_dc(&mut self, handle: winapi::shared::windef::HWND, content_area: Option<core::Rect>, offset_pos: Option<core::Point>) -> opencv::Result<core::Mat> {
        if self.width != self.prev_image.cols() || self.height != self.prev_image.rows() {
            // サイズ変更があると作成し直す
            return self.get_mat_from_hwnd(handle, content_area, offset_pos);
        }
        
        unsafe {
            let dc_handle = GetDC(handle);

            let capture_area = match content_area {
                Some(rect) => rect,
                None => core::Rect { x:0, y:0, width: self.width, height: self.height },
            };
    
            if let Some(pos) = offset_pos {
                BitBlt(self.compatibility_dc_handle, 0, 0, capture_area.width, capture_area.height,
                    dc_handle, pos.x + capture_area.x, pos.y + capture_area.y, SRCCOPY);
            } else {
                BitBlt(self.compatibility_dc_handle, 0, 0, capture_area.width, capture_area.height,
                    dc_handle, capture_area.x, capture_area.y, SRCCOPY);
            }

            let bitmap_handle = GetCurrentObject(self.compatibility_dc_handle, OBJ_BITMAP) as HBITMAP;
            GetObjectW(bitmap_handle as HANDLE, std::mem::size_of::<BITMAP>() as i32, &mut self.bitmap as PBITMAP as LPVOID);

            let mut bitmap_info: BITMAPINFO = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: self.width, biHeight: -self.height,
                    biPlanes: 1, biBitCount: self.bitmap.bmBitsPixel, biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            /*
            // Mat の公式?の変換方法なのに、セグフォで落ちる (as_raw_mit_Mat() で move してる？)
            GetDIBits(self.compatibility_dc_handle, bitmap_handle, 0, self.height as u32,
                self.prev_image.as_raw_mut_Mat() as LPVOID, &mut bitmap_info, DIB_RGB_COLORS);
            */
            // prev_image に割り当てていたポインタへ直接コピーする(get_mat_from_hwndで確保したやつ)
            GetDIBits(self.compatibility_dc_handle, bitmap_handle, 0, self.height as u32,
                self.pixel_buffer_pointer, &mut bitmap_info, DIB_RGB_COLORS);

            ReleaseDC(handle, dc_handle);
        }

        Ok(match content_area {
            Some(rect) => core::Mat::roi(&self.prev_image.clone(), core::Rect {x:0, y:0, width:rect.width, height:rect.height} )?,
            None => self.prev_image.clone(),
        })
    }

    /// HWND から HDC を取得して Mat に変換する
    fn get_mat_from_hwnd(&mut self, handle: winapi::shared::windef::HWND, content_area: Option<core::Rect>, offset_pos: Option<core::Point>)
        -> opencv::Result<core::Mat>
    {
        self.release();
            
        // HWND から HDC -> 互換 DC 作成 -> 互換 DC から BITMAP 取得 -> ピクセル情報を元に Mat 作成
        unsafe {
            let dc_handle = GetDC(handle);

            // BITMAP 情報取得(BITMAP経由でしかデスクトップの時の全体の大きさを取得する方法がわからなかった(!GetWindowRect, !GetClientRect, !GetDeviceCaps))
            // しかもここの bmBits をコピーしたらどっかわからん領域をコピーしてしまう(見た感じ過去に表示されていた内容からメモリが更新されてない？？？)
            let bitmap_handle = GetCurrentObject(dc_handle, OBJ_BITMAP) as HBITMAP;
            if 0 == GetObjectW(bitmap_handle as HANDLE, std::mem::size_of::<BITMAP>() as i32, &mut self.bitmap as PBITMAP as LPVOID) {
                // ビットマップ情報取得失敗, 権限が足りないとか、GPUとかで直接描画してるとかだと取得できないっぽい
                return Err(opencv::Error::new(opencv::core::StsError, "not get object bitmap.".to_string()));
            }

            self.width = self.bitmap.bmWidth;
            self.height = self.bitmap.bmHeight;

            // ので一度どっかにコピーするためにほげほげ
            let mut bitmap_info: BITMAPINFO = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: self.width, biHeight: -self.height,
                    biPlanes: 1, biBitCount: self.bitmap.bmBitsPixel, biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };

            let channels = self.bitmap.bmBitsPixel as i32 / 8;
            self.size = (self.width * self.height * channels) as usize;
            let mut pixel_buffer = Vec::<u8>::with_capacity(self.size);
            pixel_buffer.set_len(self.size);
            self.pixel_buffer_pointer = Box::into_raw(pixel_buffer.into_boxed_slice()) as LPVOID;

            let bitmap_handle = CreateDIBSection(dc_handle, &mut bitmap_info, DIB_RGB_COLORS,
                self.pixel_buffer_pointer as *mut LPVOID, 0 as HANDLE, 0);
            self.compatibility_dc_handle = CreateCompatibleDC(dc_handle);
            SelectObject(self.compatibility_dc_handle, bitmap_handle as HGDIOBJ);
            if let Some(pos) = offset_pos {
                BitBlt(self.compatibility_dc_handle, 0, 0, self.width, self.height, dc_handle, pos.x, pos.y, SRCCOPY);
            } else {
                BitBlt(self.compatibility_dc_handle, 0, 0, self.width, self.height, dc_handle, 0, 0, SRCCOPY);
            }

            let bitmap_handle = GetCurrentObject(self.compatibility_dc_handle, OBJ_BITMAP) as HBITMAP;
            GetObjectW(bitmap_handle as HANDLE, std::mem::size_of::<BITMAP>() as i32, &mut self.bitmap as PBITMAP as LPVOID);

            // オーバーヘッドやばいけど毎回作成する
            let temp_mat = core::Mat::new_rows_cols_with_data(
                self.height, self.width, core::CV_MAKETYPE(core::CV_8U, channels),
                self.bitmap.bmBits as LPVOID, core::Mat_AUTO_STEP
            )?;
            
            // move. content_area が指定されている場合は切り取る
            self.prev_image.release()?;
            self.prev_image = match content_area {
                Some(rect) => core::Mat::roi(&temp_mat, rect)?,
                None => temp_mat,
            };

            // メモリ開放
            ReleaseDC(handle, dc_handle);

            self.width = self.prev_image.cols();
            self.height = self.prev_image.rows();
            Ok(self.prev_image.clone())
        }
    }
}


/// Hoge をキャプチャするクラス
pub trait CaptureTrait {
    /// Mat を返す
    fn get_mat(&mut self) -> opencv::Result<core::Mat>;
}

/// ビデオキャプチャ から Mat
pub struct CaptureFromVideoDevice {
    pub video_capture: Box<dyn videoio::VideoCaptureTrait>,
    pub prev_image: core::Mat,
    pub empty_data: core::Mat,
}
impl CaptureTrait for CaptureFromVideoDevice {
    fn get_mat(&mut self) -> opencv::Result<core::Mat> {
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
    pub fn new(index: i32) -> opencv::Result<Self> {
        let mut video_capture = match videoio::VideoCapture::new(index, videoio::CAP_DSHOW) {
            Ok(video_capture) => Box::new(video_capture),
            Err(e) => {
                println!("{}", e);
                return Err(e)
            },
        };
        video_capture.set(opencv::videoio::CAP_PROP_FRAME_WIDTH, 640f64)?;
        video_capture.set(opencv::videoio::CAP_PROP_FRAME_HEIGHT, 360f64)?;
        
        Ok(Self {
            video_capture,
            prev_image: core::Mat::default(),
            empty_data: unsafe{ core::Mat::new_rows_cols(360, 640, core::CV_8UC3)? },
        })
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
        }

        device_list
    }
}

/// ウィンドウ から Mat
pub struct CaptureFromWindow {
    capture_dc: CaptureDC,
    pub win_caption: String,
    win_handle: winapi::shared::windef::HWND,
    pub prev_image: core::Mat,
    pub content_area: core::Rect,
}
impl CaptureTrait for CaptureFromWindow {
    fn get_mat(&mut self) -> opencv::Result<core::Mat> {
        if self.win_handle.is_null() {
            return Ok( unsafe{core::Mat::new_rows_cols(360, 640, core::CV_8UC4)}? );
        }

        if let Ok(mat) = self.capture_dc.get_mat(self.win_handle, Some(self.content_area), None) {
            let resize = core::Size { width: 640, height: 360 };

            if mat.cols() != resize.width || mat.rows() != resize.height {
                self.prev_image.release()?;
                imgproc::resize(&mat, &mut self.prev_image, resize, 0.0, 0.0, opencv::imgproc::INTER_LINEAR)?;
            } else {
                self.prev_image.release()?;
                self.prev_image = mat;
            }
        }
        Ok(self.prev_image.try_clone()?)
    }
}
impl CaptureFromWindow {
    pub fn new(win_caption: &str) -> opencv::Result<Self> {
        let win_handle = unsafe {
            winapi::um::winuser::FindWindowW(
                std::ptr::null_mut(),
                if win_caption.is_empty() { std::ptr::null_mut() } else { to_wchar(win_caption) }
            )
        };
        if win_handle.is_null() {
            return Err( opencv::Error::new(0, format!("window handle is null w[{:?}]", win_caption)) );
        }
        log::info!("found window handle: {}", win_handle as i64);

        // ウィンドウの大きさの設定
        let mut client_rect = winapi::shared::windef::RECT { left:0, top:0, right:0, bottom:0 };
        unsafe { winapi::um::winuser::GetClientRect(win_handle, &mut client_rect) };
        let mut content_area = core::Rect {
            x: client_rect.left, y: client_rect.top,
            width: client_rect.right, height: client_rect.bottom
        };

        // 自身を作成して、予め CaptureDC へ大きさを渡しておく
        let mut own = Self {
            win_caption: win_caption.to_string(),
            win_handle: win_handle,
            prev_image: core::Mat::default(),
            content_area: content_area,
            capture_dc: CaptureDC::default(),
        };
        own.capture_dc.width = own.content_area.width;
        own.capture_dc.height = own.content_area.height;

        // 指定された領域で探す
        let mut is_found = false;
        let mut max_match_ratio = 0.0;
        let mut ready_to_fight_scene = ReadyToFightScene::new_gray();
        let mut find_capture = |own: &mut Self, content_area: &core::Rect, | -> bool {
            let capture_image = own.get_mat().unwrap();
            if capture_image.empty().unwrap() {
                own.content_area = *content_area;
                return false;
            }

            // より精度が高いほうを選択
            ready_to_fight_scene.is_scene(&capture_image, None).unwrap();
            let scene_judgment = if ready_to_fight_scene.red_scene_judgment.prev_match_ratio < ready_to_fight_scene.grad_scene_judgment.prev_match_ratio {
                &ready_to_fight_scene.grad_scene_judgment
            } else {
                &ready_to_fight_scene.red_scene_judgment
            };

            if max_match_ratio < scene_judgment.prev_match_ratio {
                max_match_ratio = scene_judgment.prev_match_ratio;
            }

            if scene_judgment.is_near_match() {
                is_found |= true;
                log::info!("found window:{:3.3}% {:?}", scene_judgment.prev_match_ratio, own.content_area);
                return true;
            }

            false
        };

        // 微調整(大きさの変更での座標調整)
        content_area = own.content_area;
        'find_capture_resize: for y in [-1, 0, 1].iter() {
            for x in [-1, 0, 1].iter() {
                if own.content_area.x + x < 0 || own.content_area.y + y < 0 {
                    continue;
                }
                own.content_area.x = content_area.x + x;
                own.content_area.y = content_area.y + y;
                if find_capture(&mut own, &content_area) {
                    break 'find_capture_resize;
                }
                own.content_area = content_area;
            }
        }

        // 微調整(解像度の変更での調整)
        content_area = own.content_area;
        own.content_area.width = 640;
        own.content_area.height = 360;
        if !find_capture(&mut own, &content_area) {
            own.content_area = content_area;
        }
        
        if !is_found {
            return Err(opencv::Error::new(0, format!("not capture ReadyToFight. max ratio: {} < {}",
                max_match_ratio,
                ready_to_fight_scene.red_scene_judgment.get_border_match_ratio())
            ));
        }

        Ok(own)
    }

    // EnumWindows 用のコールバック関数, lparam としてクロージャをもらい、ウィンドウを列挙して渡す
    unsafe extern "system" fn enum_callback<T: FnMut(String)>(hwnd: winapi::shared::windef::HWND, lparam: isize) -> i32 {
        if winapi::um::winuser::IsWindowVisible(hwnd) == winapi::shared::minwindef::FALSE {
            return winapi::shared::minwindef::TRUE;
        }

        const BUF_SIZE: usize = 512;
        let mut win_window_buffer: [u16; BUF_SIZE] = [0; BUF_SIZE];
        let writed_length = GetWindowTextW(hwnd, win_window_buffer.as_mut_ptr(), BUF_SIZE as i32);
        if 0 < writed_length {
            let window_text_func = &mut *(lparam as *mut T);
            window_text_func(String::from_utf16_lossy(&win_window_buffer[0..writed_length as usize]));
        }

        winapi::shared::minwindef::TRUE
    }

    // EnumWindows のコールバックをクロージャで呼べるようにするためのラッパー
    fn enum_windows<T: FnMut(String)>(mut window_text_func: T) {
        unsafe {
            EnumWindows(Some(Self::enum_callback::<T>), &mut window_text_func as *mut _ as isize);
        }
    }

    /// ウィンドウ名を列挙して返す
    pub fn get_window_list() -> Vec<String> {
        let mut window_list: Vec<String> = Vec::new();
        Self::enum_windows(|win_caption: String| {
            window_list.push(win_caption);
        });

        window_list
    }
}

/// デスクトップ から Mat
pub struct CaptureFromDesktop {
    capture_dc: CaptureDC,
    win_handle: winapi::shared::windef::HWND,
    pub prev_image: core::Mat,
    monitor_lefttop: core::Point,
    content_area: core::Rect,
    resolution: i32,
}
impl CaptureTrait for CaptureFromDesktop {
    fn get_mat(&mut self) -> opencv::Result<core::Mat> {
        if let Ok(mat) = self.capture_dc.get_mat(self.win_handle, Some(self.content_area), Some(self.monitor_lefttop)) {
            let resize = core::Size {
                width: (40.0 / self.resolution as f32 * mat.cols() as f32) as i32,
                height: (40.0 / self.resolution as f32 * mat.rows() as f32) as i32
            };

            if mat.cols() != resize.width || mat.rows() != resize.height {
                imgproc::resize(&mat, &mut self.prev_image, resize, 0.0, 0.0, opencv::imgproc::INTER_LINEAR).unwrap();
            } else {
                self.prev_image = mat;
            }
        }
        Ok(self.prev_image.try_clone()?)
    }
}
impl CaptureFromDesktop {
    pub fn new() -> opencv::Result<Self> {
        // デスクトップ画面から ReadyToFight を検出して位置を特定する
        log::info!("finding capture area from desktop...");
        let desktop_handle = 0 as winapi::shared::windef::HWND;

        // モニターの左上の座標を取得
        let mut monitor_lefttop = core::Point { x:0, y:0 };
        unsafe {
            monitor_lefttop.x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            monitor_lefttop.y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        }

        // 解像度の特定, よく使われる解像度を先に指定する (640x360 未満は扱わない, FHDまで)
        let mut capture_dc = CaptureDC::default();
        let mut ready_to_fight_scene = ReadyToFightScene::new_trans();
        let mut content_area = core::Rect { x: 0, y: 0, width: 0, height: 0 };
        let mut find_resolution: i32 = 40;
        let base_resolution = core::Size { width: 16, height: 9 };
        let resolution_list = vec![40, 44, 50, 53, 60, 64, 70, 80, 90, 96, 100, 110, 120];
        let mut found = false;
        for resolution in resolution_list {
            let gui_status = format!("\rfinding dpi=[{}]", resolution);
            
            let mut mat = match capture_dc.get_mat(desktop_handle, None, Some(monitor_lefttop)) {
                Ok(v) => v,
                Err(_) => continue,
            };
 
            let mut resized_mat = core::Mat::default();
            let red_magnification = 40.0 / resolution as f32;
            let exp_magnification = resolution as f32 / 40.0;

            let resize = core::Size {
                width: (red_magnification * mat.cols() as f32) as i32,
                height: (red_magnification * mat.rows() as f32) as i32
            };
            imgproc::resize(&mat, &mut resized_mat, resize, 0.0, 0.0, imgproc::INTER_LINEAR).unwrap();

            ready_to_fight_scene.is_scene(&resized_mat, None).unwrap();
            let scene_judgment;
            if ready_to_fight_scene.red_scene_judgment.prev_match_ratio < ready_to_fight_scene.grad_scene_judgment.prev_match_ratio {
                scene_judgment = &ready_to_fight_scene.grad_scene_judgment;
            } else {
                scene_judgment = &ready_to_fight_scene.red_scene_judgment;
            }
            content_area.x = (exp_magnification * scene_judgment.prev_match_point.x as f32) as i32;
            content_area.y = (exp_magnification * scene_judgment.prev_match_point.y as f32) as i32;
            content_area.width = resolution * base_resolution.width;
            content_area.height = resolution * base_resolution.height;
            if scene_judgment.is_near_match() {
                log::info!("found dpi:{} {:?}", resolution, content_area);
                found = true;

                find_resolution = resolution;
                imgproc::rectangle(&mut mat,
                    core::Rect { x:content_area.x-2, y:content_area.y-2, width:content_area.width+4, height:content_area.height+4 },
                    core::Scalar::new(0.0, 0.0, 255.0, 255.0), 1, imgproc::LINE_8, 0).unwrap();
                imgcodecs::imwrite("found_capture_area.png", &mat, &core::Vector::from(vec![])).unwrap();
                break;
            }
        }
        if !found {
            return Err(opencv::Error::new(0, "not found capture area.".to_string()));
        }

        Ok(Self {
            capture_dc: capture_dc,
            win_handle: desktop_handle,
            prev_image: core::Mat::default(),
            content_area: content_area,
            resolution: find_resolution,
            monitor_lefttop: monitor_lefttop
        })
    }
}

/// デフォルト用の空 Mat
pub struct CaptureFromEmpty {
    pub prev_image: core::Mat,
}
impl CaptureTrait for CaptureFromEmpty {
    fn get_mat(&mut self) -> opencv::Result<core::Mat> { Ok(self.prev_image.try_clone()?) }
}
impl CaptureFromEmpty {
    pub fn new() -> opencv::Result<Self> {
        Ok(Self {
            prev_image: imgcodecs::imread("resource/loading_color.png", imgcodecs::IMREAD_UNCHANGED).unwrap()
        })
    }
}


/// コーデックを保持するクラス
struct Codec {
    backend: i32,
    fourcc: i32,
    file_name: Option<String>,
}
impl AsRef<Codec> for Codec {
    fn as_ref(&self) -> &Codec { self }
}
impl Codec {
    /// インストールされているコーデックを照合して初期化
    pub fn find_codec() -> Self {
        log::info!("find installed codec... {:?}", videoio::get_writer_backends());
        let fourcc_list = [b"HEVC", b"H265", b"X264", b"FMP4", b"ESDS", b"MP4V", b"MJPG"];
        let extension_list = ["mp4", "avi"];
        let mut writer: videoio::VideoWriter = videoio::VideoWriter::default().unwrap();
        let mut codec = Self {
            backend: videoio::CAP_ANY,
            fourcc: -1,
            file_name: None
        };
        let mut file_name = String::from("");
        // 環境によってインストールされている バックエンド/コーデック/対応する拡張子 が違うので特定
        'find_backends: for backend in videoio::get_writer_backends().unwrap() {
            if videoio::CAP_IMAGES == backend as i32 {
                // CAP_IMAGES だけ opencv のほうでエラーが出るので除外
                // (Rust 側の Error文でないので抑制できないし、消せないし邪魔)
                // error: (-5:Bad argument) CAP_IMAGES: can't find starting number (in the name of file): temp.avi in function 'cv::icvExtractPattern'
                continue;
            }

            for fourcc in fourcc_list.to_vec() {
                for extension in extension_list.to_vec() {
                    codec.backend = backend as i32;
                    codec.fourcc = i32::from_ne_bytes(*fourcc);
                    file_name = format!("temp.{}", extension);
    
                    let ret = writer.open_with_backend(
                        &file_name, codec.backend, codec.fourcc,
                        15.0, core::Size{width: 640, height: 360}, true
                    ).unwrap_or(false);
                    if ret {
                        log::info!("codec initialized: {:?} ({:?}) to {:?}", backend, std::str::from_utf8(fourcc).unwrap(), &file_name);
                        break 'find_backends;
                    }

                    codec.backend = videoio::CAP_ANY;
                    codec.fourcc = -1;
                    file_name = "temp.avi".to_string();
                }
            }
        }
        writer.release().ok();

        codec.file_name = Some(file_name);
        codec
    }

    /// コーデック情報に基づいて VideoWriter を初期化する
    pub fn open_writer_with_codec(&self, writer: &mut videoio::VideoWriter) -> opencv::Result<bool> {
        writer.open_with_backend(
            &self.file_name.clone().unwrap(), self.backend, self.fourcc,
            15.0, core::Size{width: 640, height: 360}, true
        )
    }

    /// コーデック情報に基づいて VideoWriter を初期化する
    pub fn open_reader_with_codec(&self, reader: &mut videoio::VideoCapture) -> opencv::Result<bool> {
        reader.open_file(&self.file_name.clone().unwrap(), self.backend)
    }
}
/// シングルトンでコーデックを保持するため
struct WrappedCodec {
    codecs: Option<Codec>,
}
impl WrappedCodec {
    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    fn get(&mut self) -> &Codec {
        if self.codecs.is_none() {
            self.codecs = Some(Codec::find_codec());
        }
        self.codecs.as_ref().unwrap()
    }
}
static mut CODECS: WrappedCodec = WrappedCodec {
    codecs: None
};

use std::time::{Duration, Instant};
/// キャプチャ用のバッファ(動画ファイル[temp.*])を管理するクラス
pub struct CaptureFrameStore {
	writer: videoio::VideoWriter,
    reader: videoio::VideoCapture,

    prev_image: core::Mat,
    filled_by_frame: bool,
    pub recoded_frame: i32,

    recoding_start_time: Option<Instant>,
    recoding_end_time: Option<Instant>,
    recoding_need_frame: Option<i32>,
}
impl Default for CaptureFrameStore {
    fn default() -> Self {
        Self {
            writer: videoio::VideoWriter::default().unwrap(),
            reader: videoio::VideoCapture::default().unwrap(),

            prev_image: core::Mat::default(),
            filled_by_frame: false,
            recoded_frame: 0,

            recoding_start_time: None,
            recoding_end_time: None,
            recoding_need_frame: None,
        }
    }
}
impl CaptureFrameStore {
    // 録画開始時にする処理
    fn recoding_initialize(&mut self) -> opencv::Result<()> {
        self.recoding_start_time = None;
        self.recoding_end_time = None;
        self.recoding_need_frame = None;
        self.recoded_frame = 0;
        self.filled_by_frame = false;

        if self.reader.is_opened()? {
            // 開放しないと reader と writer で同じコーデックを使おうとする時に初期化できなくて怒られる
            self.reader.release()?;
        }

        if !unsafe{ CODECS.get() }.open_writer_with_codec(&mut self.writer).unwrap_or(false) {
            return Err(opencv::Error::new( 0, "not found Codec for [*.mp4 or *.avi]. maybe: you install any Codec for your PC".to_string() ));
        }

        Ok(())
    }

    // 録画再生時前にする処理 (録画終了時に呼ばれる)
    fn replay_initialize(&mut self) -> opencv::Result<()> {
        if self.reader.is_opened()? {
            return Ok(());
        }

        if self.writer.is_opened()? {
            // 開放しないと reader と writer で同じコーデックを使おうとする時に初期化できなくて怒られる
            self.writer.release()?;
        }

        if !unsafe{ CODECS.get() }.open_reader_with_codec(&mut self.reader).unwrap_or(false) {
            return Err(opencv::Error::new( 0, "not initialized video reader. maybe: playing temp video?".to_string() ));
        }

        self.filled_by_frame = true;

        Ok(())
    }

    // 録画再生終了時にする処理
    fn replay_finalize(&mut self) -> opencv::Result<()> {
        if !self.reader.is_opened()? {
            return Ok(());
        }

        self.reader.release()?;
        self.filled_by_frame = false;

        Ok(())
    }

    /// 録画が開始してるか
    pub fn is_recoding_started(&self) -> bool {
        if !self.writer.is_opened().unwrap() {
            return false;
        }

        if let Some(recoding_start_time) = self.recoding_start_time {
            return recoding_start_time <= Instant::now();
        }
        if let Some(recoding_need_frame) = self.recoding_need_frame {
            return 0 < recoding_need_frame;
        }
        
        // 開始時間が指定されていない or 必要なフレームが指定されていない
        false
    }

    /// 録画が終わってるか
    pub fn is_recoding_end(&self) -> bool {
        if !self.is_recoding_started() {
            return false;
        }

        if let Some(recoding_end_time) = self.recoding_end_time {
            return recoding_end_time <= Instant::now();
        }
        if let Some(recoding_need_frame) = self.recoding_need_frame {
            return recoding_need_frame <= self.recoded_frame;
        }
        
        // 終了時間より現在が後 or 必要なフレームが溜まった
        false
    }

    /// リプレイが終わってるか
    pub fn is_replay_end(&self) -> bool {
        // self.recoded_frame が 0 に減算されきっていて、まだ recoding_hoge が初期化されていないと true
        if 0 < self.recoded_frame {
            return false;
        }

        self.recoding_start_time.is_some() || self.recoding_need_frame.is_some()
    }

    /// 録画が終わり、必要なフレームが満たされたか
    pub fn is_filled(&self) -> bool {
        self.filled_by_frame
    }

    /// now から end_time_duration 秒後まで録画しつづける
    pub fn start_recoding_by_time(&mut self, end_time_duration: Duration) {
        if self.is_recoding_started() {
            return;
        }

        self.recoding_initialize().unwrap();
        self.recoding_start_time = Some(Instant::now());
        self.recoding_end_time = Some(self.recoding_start_time.unwrap() + end_time_duration);
    }

    /// 必要なフレームがたまるまで録画しつづける
    pub fn start_recoding_by_frame(&mut self, need_frame: i32) {
        if self.is_recoding_started() {
            return;
        }

        self.recoding_initialize().unwrap();
        self.recoding_need_frame = Some(need_frame);
        self.recoded_frame = 0;
    }

    /// capture_image を[条件まで]ファイルへ録画する
    /// 条件 : start_recoding_by_time or start_recoding_by_frame
    pub fn recoding_frame(&mut self, capture_image: &core::Mat) -> opencv::Result<()> {
        if self.is_filled() {
            // フレームで満たされると reader を開いて replay_frame で空になるまで放置
            return Ok(());
        }
        if !self.is_recoding_started() {
            return Ok(());
        }

        // 動画を一度通すと BGR 同士で処理されなくなるので、色変換を行う。BGR to RGB
        let mut changed_color_capture_image = core::Mat::default();
        imgproc::cvt_color(&capture_image, &mut changed_color_capture_image, opencv::imgproc::COLOR_BGRA2RGBA, 0)?;

        self.writer.write(&changed_color_capture_image)?;
        self.recoded_frame += 1;

        if self.is_recoding_end() {
            self.replay_initialize()?;
        }
        Ok(())
    }

    /// 録画されたシーンを再生する, get_replay_frame: フレームを受け取るクロージャ
    /// replay_scene( |frame| { /* frame hogehoge */ } )?;
    pub fn replay_frame<F>(&mut self, mut get_replay_frame: F) -> opencv::Result<bool>
        where F: FnMut(&core::Mat) -> opencv::Result<bool>
    {
        if !self.reader.is_opened()? || !self.is_filled() || !self.reader.grab()? {
            // reader が開かれていない(録画されていない) or フレームで満たされていない or 準備が整ってない
            return Ok(false);
        }

        // 1 frame 取得
        self.reader.retrieve(&mut self.prev_image, 0)?;

        // フレームに対して行う処理
        if get_replay_frame(&self.prev_image)? {
            // true が帰ると replay を終了する
            self.recoded_frame = 1;
        }

        if 0 < self.recoded_frame {
            self.recoded_frame -= 1;
            if 0 == self.recoded_frame {
                self.replay_finalize()?;
            }
        }

        Ok(true)
    }
}

#![cfg(windows)]
#![windows_subsystem = "windows"]

use opencv::{
    core,
    imgcodecs,
    imgproc,
    prelude::*,
    videoio,
};
use winapi::shared::minwindef::LPVOID;
use winapi::shared::windef::*;
use winapi::um::wingdi::*;
use winapi::um::winnt::HANDLE;
use winapi::um::winuser::*;

use crate::gui::GUI;
use crate::scene::{ReadyToFightScene, SceneTrait};


/// &str -> WCHAR
fn to_wchar(value: &str) -> *mut winapi::ctypes::wchar_t {
    use std::os::windows::ffi::OsStrExt;

    let mut vec16 :Vec<u16> = std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    vec16.as_mut_ptr()
}

/// Mat,DCを保持するクラス。毎回 Mat を作成するのはやはりメモリコストが高すぎた。
struct CaptureDC {
    prev_image: core::Mat,
    compatibility_dc_handle: HDC,
    pixel_buffer_pointer: LPVOID,
    size: usize,
    width: i32, height: i32,
}
impl Default for CaptureDC {
    fn default() -> Self {
        Self {
            prev_image: core::Mat::default(),
            compatibility_dc_handle: 0 as HDC,
            pixel_buffer_pointer: 0 as LPVOID,
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
            let mut bitmap: BITMAP = std::mem::zeroed();
            GetObjectW(bitmap_handle as HANDLE, std::mem::size_of::<BITMAP>() as i32, &mut bitmap as PBITMAP as LPVOID);

            let mut bitmap_info: BITMAPINFO = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: self.width, biHeight: -self.height,
                    biPlanes: 1, biBitCount: bitmap.bmBitsPixel, biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            GetDIBits(self.compatibility_dc_handle, bitmap_handle, 0, self.height as u32,
                self.prev_image.as_raw_mut_Mat() as LPVOID, &mut bitmap_info, DIB_RGB_COLORS);

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
            let mut bitmap: BITMAP = std::mem::zeroed();
            GetObjectW(bitmap_handle as HANDLE, std::mem::size_of::<BITMAP>() as i32, &mut bitmap as PBITMAP as LPVOID);
            self.width = bitmap.bmWidth;
            self.height = bitmap.bmHeight;

            // ので一度どっかにコピーするためにほげほげ
            let mut bitmap_info: BITMAPINFO = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: self.width, biHeight: -self.height,
                    biPlanes: 1, biBitCount: bitmap.bmBitsPixel, biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };

            let channels = bitmap.bmBitsPixel as i32 / 8;
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
            let mut bitmap: BITMAP = std::mem::zeroed();
            GetObjectW(bitmap_handle as HANDLE, std::mem::size_of::<BITMAP>() as i32, &mut bitmap as PBITMAP as LPVOID);

            // オーバーヘッドやばいけど毎回作成する
            let mut temp_mat = core::Mat::new_rows_cols_with_data(
                self.height, self.width, core::CV_MAKETYPE(core::CV_8U, channels),
                bitmap.bmBits as LPVOID, core::Mat_AUTO_STEP
            )?;
            
            // move. content_area が指定されている場合は切り取る
            self.prev_image = match content_area {
                Some(rect) => core::Mat::roi(&temp_mat, rect)?,
                None => temp_mat,
            };

            // メモリ開放
            ReleaseDC(handle, dc_handle);

            Ok(self.prev_image.clone())
        }
    }
}


/// Hoge をキャプチャするクラス
pub trait CaptureTrait {
    /// Mat を返す
    fn get_mat(&mut self) -> opencv::Result<core::Mat>;
}

pub struct CaptureNone {
    pub prev_image: core::Mat,
}
impl Default for CaptureNone {
    fn default() -> Self { Self {
        prev_image: core::Mat::default()
    } }
}
impl CaptureTrait for CaptureNone {
    fn get_mat(&mut self) -> opencv::Result<core::Mat> { Ok(self.prev_image.try_clone()?) }
}

/// ビデオキャプチャ から Mat
pub struct CaptureFromVideoDevice {
    pub video_capture: Box<dyn videoio::VideoCaptureTrait>,
    pub prev_image: core::Mat,
}
impl Default for CaptureFromVideoDevice {
    fn default() -> Self {
        CaptureFromVideoDevice::new(0)
    }
}
impl CaptureTrait for CaptureFromVideoDevice {
    fn get_mat(&mut self) -> opencv::Result<core::Mat> {
        // Err の場合 Mat::default を返してほしい
        self.video_capture.grab()?;
        self.video_capture.retrieve(&mut self.prev_image, 0)?;

        Ok(self.prev_image.try_clone()?)
    }
}
impl CaptureFromVideoDevice {
    pub fn new(index: i32) -> Self {
        let mut own = Self {
            video_capture: Box::new(videoio::VideoCapture::new(index, videoio::CAP_DSHOW).unwrap()),
            prev_image: core::Mat::default(),
        };
        own.video_capture.set(opencv::videoio::CAP_PROP_FRAME_WIDTH, 640f64).unwrap();
        own.video_capture.set(opencv::videoio::CAP_PROP_FRAME_HEIGHT, 360f64).unwrap();

        own
    }
}

/// ウィンドウ から Mat
pub struct CaptureFromWindow {
    capture_dc: CaptureDC,
    pub win_caption: String,
    pub win_class: String,
    win_handle: winapi::shared::windef::HWND,
    pub prev_image: core::Mat,
    pub content_area: Option<core::Rect>,
}
impl Default for CaptureFromWindow {
    fn default() -> Self {
        CaptureFromWindow::new("", "")
    }
}
impl CaptureTrait for CaptureFromWindow {
    fn get_mat(&mut self) -> opencv::Result<core::Mat> {
        if let Ok(mat) = self.capture_dc.get_mat(self.win_handle, None, None) {
            if self.win_handle.is_null() {
                return Err( opencv::Error::new(0, "window handle is null".to_string()) );
            }

            self.prev_image.release()?;
            self.prev_image = mat;
        }
        Ok(self.prev_image.try_clone()?)
    }
}
impl CaptureFromWindow {
    pub fn new(win_caption: &str, win_class: &str) -> Self {
        let win_handle = unsafe {
            winapi::um::winuser::FindWindowW(
                if win_class.is_empty() { std::ptr::null_mut() } else { to_wchar(win_class) },
                if win_caption.is_empty() { std::ptr::null_mut() } else { to_wchar(win_caption) }
            )
        };

        let content_area = if win_handle.is_null() {
            None
        } else {
            let mut client_rect = winapi::shared::windef::RECT { left:0, top:0, right:0, bottom:0 };
            unsafe { winapi::um::winuser::GetClientRect(win_handle, &mut client_rect) };

            Some(core::Rect {
                x: client_rect.left, y: client_rect.top, width: client_rect.right, height: client_rect.bottom
            })
        };

        Self {
            win_caption: win_caption.to_string(),
            win_class: win_class.to_string(),
            win_handle: win_handle,
            prev_image: core::Mat::default(),
            content_area: content_area,
            capture_dc: CaptureDC::default(),
        }
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
impl Default for CaptureFromDesktop {
    fn default() -> Self {
        CaptureFromDesktop::new()
    }
}
impl CaptureTrait for CaptureFromDesktop {
    fn get_mat(&mut self) -> opencv::Result<core::Mat> {
        if let Ok(mat) = self.capture_dc.get_mat(self.win_handle, Some(self.content_area), Some(self.monitor_lefttop)) {
            let base_resolution = core::Size { width: 16, height: 9 };
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
    pub fn new() -> Self {
        // デスクトップ画面から ReadyToFight を検出して位置を特定する
        println!("finding capture area from desktop...");
        let desktop_handle = 0 as winapi::shared::windef::HWND;
        let mut ready_to_fight_scene = ReadyToFightScene::new_trans();
        let mut content_area = core::Rect { x: 0, y: 0, width: 0, height: 0 };

        // モニターの左上の座標を取得
        let mut monitor_lefttop = core::Point { x:0, y:0 };
        unsafe {
            monitor_lefttop.x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            monitor_lefttop.y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        }

        // 解像度の特定, よく使われる解像度を先に指定する (640x360 未満は扱わない, FHDまで)
        let mut capture_dc = CaptureDC::default();
        let mut find_resolution: i32 = 40;
        let base_resolution = core::Size { width: 16, height: 9 };
        let mut resolution_list = vec![40, 53, 80, 96, 100, 120];
        resolution_list.extend( (41..53).collect::<Vec<i32>>() );   // Vec が slice に対する ops::AddAssign 実装してないってマ？？？
        resolution_list.extend( (54..80).collect::<Vec<i32>>() );
        resolution_list.extend( (81..96).collect::<Vec<i32>>() );
        resolution_list.extend( (97..100).collect::<Vec<i32>>() );
        resolution_list.extend( (101..120).collect::<Vec<i32>>() );
        for resolution in resolution_list {
            println!("\r{}...", resolution);
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

            ready_to_fight_scene.is_scene(&resized_mat).unwrap();
            let mut scene_judgment;
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
                println!("found dpi:{} {:?}", resolution, content_area);

                find_resolution = resolution;
                imgproc::rectangle(&mut mat, content_area, core::Scalar::new(0.0, 0.0, 255.0, 255.0), 3, imgproc::LINE_8, 0).unwrap();
                imgcodecs::imwrite("found_capture_area.png", &mat, &core::Vector::from(vec![])).unwrap();
                break;
            }
        }

        Self {
            capture_dc: capture_dc,
            win_handle: desktop_handle,
            prev_image: core::Mat::default(),
            content_area: content_area,
            resolution: find_resolution,
            monitor_lefttop: monitor_lefttop
        }
    }
}



use std::time::{Duration, Instant};
pub struct CaptureFrameStore {
    backend: i32,
    fourcc: i32,
    file_name: String,

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
            file_name: "temp.avi".to_string(),
            backend: videoio::CAP_ANY,
            fourcc: -1,

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
        
        // 環境によってインストールされている バックエンド/コーデック が違うので特定
        println!("find installed codec... {:?}", videoio::get_writer_backends());
        let fourcc_list = [b"HEVC", b"H265", b"X264", b"FMP4", b"ESDS", b"MP4V", b"MJPG"];
        let extension_list = ["mp4", "avi"];
        'find_backends: for backend in videoio::get_writer_backends()? {
            if videoio::CAP_IMAGES == backend as i32 {
                // CAP_IMAGES だけ opencv のほうでエラーが出るので除外
                // (Rust 側の Error文でないので抑制できないし、消せないし邪魔)
                continue;
            }

            for fourcc in fourcc_list.to_vec() {
                for extension in extension_list.to_vec() {
                    self.backend = backend as i32;
                    self.fourcc = i32::from_ne_bytes(*fourcc);
                    self.file_name = format!("temp.{}", extension);
    
                    let ret = match self.writer.open_with_backend(
                        &self.file_name, self.backend, self.fourcc,
                        15.0, core::Size{width: 640, height: 360}, true
                    ) {
                        Ok(v) => v,
                        Err(_) => false,
                    };
                    if ret {
                        println!("video writer initialize: {:?} ({:?}) to {:?}", backend, std::str::from_utf8(fourcc).unwrap(), self.file_name);
                        break 'find_backends;
                    }

                    self.backend = videoio::CAP_ANY;
                    self.fourcc = -1;
                    self.file_name = "temp.avi".to_string();
                }
            }
        }

        Ok(())
    }

    // 録画再生終了時にする処理
    fn replay_finalize(&mut self) -> opencv::Result<()> {
        if !self.is_filled() {
            return Ok(());
        }

        self.filled_by_frame = false;

        // reader を空にして 満たされていないことにする
        self.reader.release()?;

        Ok(())
    }

    // 録画が開始してるか
    fn is_recoding_started(&self) -> bool {
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
    // self.recoded_frame が 0 に減算されきっていて、まだ recoding_hoge が初期化されていないと true
    pub fn is_replay_end(&self) -> bool {
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
        self.recoding_initialize().unwrap();
        self.recoding_start_time = Some(Instant::now());
        self.recoding_end_time = Some(self.recoding_start_time.unwrap() + end_time_duration);
    }

    /// 必要なフレームがたまるまで録画しつづける
    pub fn start_recoding_by_frame(&mut self, need_frame: i32) {
        self.recoding_initialize().unwrap();
        self.recoding_need_frame = Some(need_frame);
        self.recoded_frame = 0;
    }

    /// capture_image を[条件まで]ファイルへ録画する
    /// 条件 : start_recoding_by_time or start_recoding_by_frame
    pub fn recoding_frame(&mut self, capture_image: &core::Mat) -> opencv::Result<()> {
        if self.is_filled() {
            // フレームで満たされると reader を開いて replay_frame で空になるまで放置
            if self.reader.is_opened()? {
                return Ok(());
            }
            self.writer.release()?;
            let ret = self.reader.open_file(&self.file_name, self.backend)?;
            println!("video reader initialize: {}", ret);
            return Ok(());
        }
        if !self.is_recoding_started() {
            return Ok(());
        }

        self.writer.write(capture_image)?;
        self.recoded_frame += 1;

        if self.is_recoding_end() {
            self.filled_by_frame = true;
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



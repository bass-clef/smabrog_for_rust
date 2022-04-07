use super::*;

/// Mat,DCを保持するクラス。毎回 Mat を作成するのはやはりメモリコストが高すぎた。
pub struct CaptureDC {
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
    pub fn release(&mut self) {
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
    pub fn get_mat(&mut self, handle: winapi::shared::windef::HWND, content_area: Option<core::Rect>, offset_pos: Option<core::Point>) -> opencv::Result<core::Mat> {
        if self.compatibility_dc_handle.is_null() {
            self.get_mat_from_hwnd(handle, content_area, offset_pos)
        } else {
            self.get_mat_from_dc(handle, content_area, offset_pos)
        }
    }

    /// 既に作成してある互換 DC に HWND -> HDC から取得して,メモリコピーして Mat を返す
    pub fn get_mat_from_dc(&mut self, handle: winapi::shared::windef::HWND, content_area: Option<core::Rect>, offset_pos: Option<core::Point>) -> opencv::Result<core::Mat> {
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
    pub fn get_mat_from_hwnd(&mut self, handle: winapi::shared::windef::HWND, content_area: Option<core::Rect>, offset_pos: Option<core::Point>)
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

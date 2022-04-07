use super::*;

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
    fn get_mat(&mut self) -> anyhow::Result<core::Mat> {
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

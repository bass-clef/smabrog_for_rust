use super::*;

/// Hoge をキャプチャするにあたって必要な情報を保持する構造体
#[derive(Debug)]
pub struct CaptureBase {
    pub prev_image: core::Mat,
    pub dummy_data: core::Mat,
    pub resolution: i32,
    pub content_area: core::Rect,
    pub offset_pos: core::Point,
    is_resize: bool,
}
impl CaptureBase {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            prev_image: core::Mat::default(),
            dummy_data: unsafe{ core::Mat::new_rows_cols(360, 640, core::CV_8UC3)? },
            resolution: 40, // 16:9 * 40 == 640:360
            content_area: core::Rect::new(0, 0, 640, 360),
            offset_pos: core::Point::new(0, 0),
            is_resize: false,
        })
    }

    /// mat を与えると ReadyToFight を検出して、解像度, 大きさ, 座標 を自動で設定して作成する
    pub fn new_from_some_types_mat(mat: core::Mat) -> anyhow::Result<Self> {
        let mut own = Self::new()?;
        let mut ready_to_fight_scene = ReadyToFightScene::new_trans();

        let mut match_ready_to_fight = |image: &core::Mat| -> anyhow::Result<(f64, core::Point)> {
            if image.empty() {
                anyhow::bail!("not found ReadyToFight.");
            }
            if ready_to_fight_scene.is_scene(image, None)? {
                let scene_judgment = ready_to_fight_scene.get_prev_match().unwrap();
    
                Ok((scene_judgment.prev_match_ratio, scene_judgment.prev_match_point))
            } else {
                anyhow::bail!("not found ReadyToFight.");
            }
        };

        // 一致しない理由があるので mat を変更して特定する
        // case.解像度が違う -> 解像度の特定が必要(ハイコスト)
        // case.座標が違う   -> 検出位置の移動が必要
        // case.大きさが違う -> リサイズが必要
        let base_resolution = core::Size { width: 16, height: 9 };
        let resolution_list = vec![40, 44, 50, 53, 60, 64, 70, 80, 90, 96, 100, 110, 120];
        let mut most_better_ratio = 0.0;
        for resolution in resolution_list {
            own.resolution = resolution;
            if let Ok((ratio, prev_match_point)) = match_ready_to_fight(&own.prev_image) {
                most_better_ratio = ratio;
                own.content_area.x = prev_match_point.x;
                own.content_area.y = prev_match_point.y;
                own.prev_image = core::Mat::roi(&mat, own.content_area)?;
                break;
            }

            let exp_magnification = resolution as f32 / 40.0;
            let resize = core::Size {
                width: (40.0 / resolution as f32 * mat.cols() as f32) as i32,
                height: (40.0 / resolution as f32 * mat.rows() as f32) as i32
            };
            imgproc::resize(&mat, &mut own.prev_image, resize, 0.0, 0.0, imgproc::INTER_LINEAR).unwrap();
            if let Ok((ratio, prev_match_point)) = match_ready_to_fight(&own.prev_image) {
                most_better_ratio = ratio;
                own.is_resize = true;
                own.content_area.x = (exp_magnification * prev_match_point.x as f32) as i32;
                own.content_area.y = (exp_magnification * prev_match_point.y as f32) as i32;
                own.content_area.width = own.resolution * base_resolution.width;
                own.content_area.height = own.resolution * base_resolution.height;
                break;
            }

        }

        // 微調整
        if mat.cols() < own.content_area.width || mat.rows() < own.content_area.height {
            let content_area = own.content_area.clone();
            let mut most_better_area = own.content_area.clone();
            for y in [-1, 0, 1].iter() {
                for x in [-1, 0, 1].iter() {
                    // 切り取る領域が画面外に出る場合は調整できない
                    if own.content_area.x + x < 0 || own.content_area.y + y < 0 || mat.cols() <= own.content_area.width + x || mat.rows() <= own.content_area.height + y {
                        continue;
                    }
    
                    own.content_area.x = content_area.x + x;
                    own.content_area.y = content_area.y + y;
                    own.convert_mat(mat.clone())?;
                    if let Ok((ratio, _)) = match_ready_to_fight(&own.prev_image) {
                        if most_better_ratio < ratio {
                            most_better_ratio = ratio;
                            most_better_area = own.content_area.clone();
                        }
                    }
                }
            }
            own.content_area = most_better_area;
        }

        if 0.0 != most_better_ratio {
            log::info!("match :{:?} {:3.3}%", own.content_area, most_better_ratio);
            Ok(own)
        } else {
            anyhow::bail!("not found ReadyToFight.");
        }
    }

    /// mat を ReadyToFight を検出できるように返す
    pub fn get_mat(&mut self, mat: core::Mat) -> anyhow::Result<core::Mat> {
        if self.convert_mat(mat.clone()).is_err() {
            return Ok(self.prev_image.clone());
        }

        if self.prev_image.empty() {
            return Ok(self.dummy_data.clone());
        }

        Ok(self.prev_image.try_clone()?)
    }

    /// 初期化した情報を元に mat を変更する
    fn convert_mat(&mut self, mat: core::Mat) -> anyhow::Result<()> {
        let mut mat = mat;
        core::Mat::roi(&mat, self.content_area)?.copy_to(&mut mat)?;

        if self.is_resize {
            let resize = core::Size {
                width: (40.0 / self.resolution as f32 * mat.cols() as f32) as i32,
                height: (40.0 / self.resolution as f32 * mat.rows() as f32) as i32
            };

            imgproc::resize(&mat, &mut self.prev_image, resize, 0.0, 0.0, opencv::imgproc::INTER_LINEAR).unwrap();
        } else {
            self.prev_image = mat;
        }

        Ok(())
    }
}

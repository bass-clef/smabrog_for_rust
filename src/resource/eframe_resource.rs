
use eframe::egui::TextureId;
use opencv::prelude::MatTraitConst;
use serde::{
    Deserialize,
    Serialize,
};
use std::collections::HashMap;

use super::*;


/// 画像とかも入ったリソース
pub struct SmashbrosResource {
    pub version: String,
    pub character_list: HashMap<String, String>,
    pub icon_list: HashMap<String, TextureId>,
    pub order_image_list: HashMap<i32, TextureId>,
    pub image_size_list: HashMap<TextureId, eframe::egui::Vec2>,
    pub i18n_convert_list: HashMap<String, String>,
}
impl SmashbrosResource {
    pub fn new(frame: &mut eframe::epi::Frame<'_>) -> Self {
        let text = SmashbrosResourceText::new();
        let mut image_size_list = HashMap::new();
        let mut icon_list: HashMap<String, TextureId> = HashMap::new();
        for (character_name, file_name) in text.icon_list.iter() {
            let (texture_id, size) = SmashbrosResource::get_texture_id(&format!("icon/{}", file_name), frame);
            icon_list.insert(character_name.to_string(), texture_id);
            image_size_list.insert(texture_id, size);
        }

        let mut order_image_list: HashMap<i32, TextureId> = HashMap::new();
        for order in 1..=4 {
            let (texture_id, size) = SmashbrosResource::get_texture_id(
                &format!("resource/result_player_order_{}_color.png", order),
                frame
            );

            order_image_list.insert(order, texture_id);
            image_size_list.insert(texture_id, size);
        }

        Self {
            version: text.version,
            character_list: text.character_list,
            icon_list,
            order_image_list,
            image_size_list,
            i18n_convert_list: text.i18n_convert_list,
        }
    }

    pub fn new_for_test() -> Self {
        let text = SmashbrosResourceText::new();
        // icon および image は GUI フレームワークからもらう frame が必要なので、test では空のままにしておく

        Self {
            version: text.version,
            character_list: text.character_list,
            icon_list: HashMap::new(),
            order_image_list: HashMap::new(),
            image_size_list: HashMap::new(),
            i18n_convert_list: text.i18n_convert_list,
        }
    }

    fn get_texture_id(path: &str, frame: &mut eframe::epi::Frame<'_>) -> (TextureId, eframe::egui::Vec2) {
        let image = opencv::imgcodecs::imread(path, opencv::imgcodecs::IMREAD_UNCHANGED).unwrap();
        let image_size = ( image.cols() * image.rows() * 4 ) as usize;
        let image_data_by_slice: &[u8] = unsafe{ std::slice::from_raw_parts(image.datastart(), image_size) };
        let pixels: Vec<_> = image_data_by_slice.to_vec()
            .chunks_exact(4)
            .map(|p| eframe::egui::Color32::from_rgba_unmultiplied(p[2], p[1], p[0], p[3]))
            .collect();

        (
            frame.tex_allocator()
                .alloc_srgba_premultiplied(
                    (image.cols() as usize, image.rows() as usize),
                    &pixels
                ),
            eframe::egui::Vec2::new(image.cols() as f32, image.rows() as f32)
        )
    }

    pub fn get_image_handle(&self, character_name: String) -> Option<TextureId> {
        if !self.icon_list.contains_key(&character_name) {
            return None;
        }

        Some(self.icon_list[&character_name].clone())
    }

    pub fn get_order_handle(&self, order: i32) -> Option<TextureId> {
        if order <= 0 || 5 <= order {
            return None;
        }

        Some(self.order_image_list[&order].clone())
    }

    pub fn get_image_size(&self, texture_id: TextureId) -> Option<eframe::egui::Vec2> {
        self.image_size_list.get(&texture_id).cloned()
    }

    // 言語の変更でのリソースの再読み込み
    pub fn change_language(&mut self) {
        let text = SmashbrosResourceText::new();
        self.character_list = text.character_list;
        self.i18n_convert_list = text.i18n_convert_list;
    }
}

/// シングルトンでリソースを保持するため
pub struct WrappedSmashbrosResource {
    smashbros_resource: Option<SmashbrosResource>
}
impl WrappedSmashbrosResource {
    pub fn init(&mut self, frame: Option<&mut eframe::epi::Frame<'_>>) {
        if self.smashbros_resource.is_none() {
            self.smashbros_resource = Some(SmashbrosResource::new( frame.unwrap() ));
        }
    }

    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    pub fn get(&mut self) -> &mut SmashbrosResource {
        if self.smashbros_resource.is_none() {
            log::error!("SmashbrosResource is not initialized. Call init() first.");
            self.smashbros_resource = Some(SmashbrosResource::new_for_test());
        }
        self.smashbros_resource.as_mut().unwrap()
    }
}
static mut SMASHBROS_RESOURCE: WrappedSmashbrosResource = WrappedSmashbrosResource {
    smashbros_resource: None,
};
pub fn smashbros_resource() -> &'static mut WrappedSmashbrosResource {
    unsafe { &mut SMASHBROS_RESOURCE }
}


// 設定ファイル
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct GUIConfig {
    pub window_x: i32,
    pub window_y: i32,
    pub capture_win_caption: String,
    pub capture_device_name: String,

    // バージョン更新でファイルに値が無い場合があるので、以下から default を追加する
    #[serde(default)]
    pub visuals: Option<eframe::egui::style::Visuals>,
    #[serde(default)]
    pub lang: Option<LanguageIdentifier>,
}
impl GUIConfig {
    const DEFAULT_CAPTION: &'static str = "smabrog";
    const CONFIG_FILE: &'static str = "config.json";

    /// 設定情報の読み込み
    pub fn load_config(&mut self, is_initalize: bool) -> anyhow::Result<()> {
        let file = std::fs::File::open(Self::CONFIG_FILE)?;
        *self = serde_json::from_reader(std::io::BufReader::new(file))?;

        if is_initalize && cfg!(target_os = "windows") {
            unsafe {
                // 位置復元
                use winapi::um::winuser;
                use crate::utils::utils::to_wchar;
                use winapi::shared::minwindef::BOOL;
                let own_handle = winuser::FindWindowW(std::ptr::null_mut(), to_wchar(Self::DEFAULT_CAPTION));
                if own_handle.is_null() {
                    return Err(anyhow::anyhow!("Not found Window."));
                }
                // リサイズされるのを期待して適当に大きくする
                winuser::MoveWindow(own_handle, self.window_x, self.window_y, 256+16, 720+39, true as BOOL);
            }
            log::info!("loaded config.");
        }

        Ok(())
    }
    /// 設定情報の保存
    pub fn save_config(&mut self, is_finalize: bool) -> Result<(), Box<dyn std::error::Error>> {
        if is_finalize && cfg!(target_os = "windows") {
            unsafe {
                // 位置復元用
                use winapi::um::winuser;
                use crate::utils::utils::to_wchar;
    
                let own_handle = winuser::FindWindowW(std::ptr::null_mut(), to_wchar(Self::DEFAULT_CAPTION));
                if !own_handle.is_null() {
                    let mut window_rect = winapi::shared::windef::RECT { left:0, top:0, right:0, bottom:0 };
                    winapi::um::winuser::GetWindowRect(own_handle, &mut window_rect);
                    self.window_x = window_rect.left;
                    self.window_y = window_rect.top;
                }
            }
            log::info!("saved config.");
        }

        use std::io::Write;
        let serialized_data = serde_json::to_string(self).unwrap();
        let mut file = std::fs::File::create(Self::CONFIG_FILE)?;
        file.write_all(serialized_data.as_bytes())?;

        Ok(())
    }
}
/// シングルトンで設定ファイルを保持するため
pub struct WrappedGUIConfig {
    gui_config: Option<GUIConfig>
}
impl WrappedGUIConfig {
    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    pub fn get(&mut self) -> &mut GUIConfig {
        if self.gui_config.is_none() {
            self.gui_config = Some(GUIConfig::default());
        }
        self.gui_config.as_mut().unwrap()
    }
}
static mut GUI_CONFIG: WrappedGUIConfig = WrappedGUIConfig {
    gui_config: None,
};
pub fn gui_config() -> &'static mut WrappedGUIConfig {
    unsafe { &mut GUI_CONFIG }
}


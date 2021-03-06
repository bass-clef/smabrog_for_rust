
use egui::TextureHandle;
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
    pub icon_list: HashMap<String, TextureHandle>,
    pub order_image_list: HashMap<i32, TextureHandle>,
    pub i18n_convert_list: HashMap<String, String>,
    pub bgm_list: HashMap<String, bool>,
}
impl SmashbrosResource {
    fn matcher(string_list: &Vec<String>, maybe_name: &String, default_name: Option<&str>) -> (String, f32) {
        let mut max_ratio = 0.0;
        let mut name = default_name.unwrap_or("");
        for string_name in string_list {
            let ratio = strsim::jaro_winkler(string_name, maybe_name) as f32;
            if max_ratio < ratio {
                max_ratio = ratio;
                name = string_name;
                if 1.0 <= max_ratio {
                    break;
                }
            }
        }
        return (name.to_string(), max_ratio);
    }

    fn get_texture_handle(path: &str, ctx: &egui::Context) -> TextureHandle {
        let image = opencv::imgcodecs::imread(path, opencv::imgcodecs::IMREAD_UNCHANGED).unwrap();
        let mut converted_image = opencv::core::Mat::default();
        opencv::imgproc::cvt_color(&image, &mut converted_image, opencv::imgproc::COLOR_BGRA2RGBA, 0).expect("failed cvt_color BGR to RGB. from get_texture_handle");

        Self::load_texture_handle(path, ctx, &converted_image)
    }

    pub fn new(ctx: &egui::Context) -> Self {
        let text = SmashbrosResourceText::new();
        let mut icon_list: HashMap<String, TextureHandle> = HashMap::new();
        for (character_name, file_name) in text.icon_list.iter() {
            let texture_handle = SmashbrosResource::get_texture_handle(&format!("icon/{}", file_name), ctx);
            icon_list.insert(character_name.to_string(), texture_handle);
        }

        let mut order_image_list: HashMap<i32, TextureHandle> = HashMap::new();
        for order in 1..=4 {
            let texture_handle = SmashbrosResource::get_texture_handle(
                &format!("resource/result_player_order_{}_color.png", order),
                ctx
            );

            order_image_list.insert(order, texture_handle);
        }

        Self {
            version: text.version,
            character_list: text.character_list,
            icon_list,
            order_image_list,
            i18n_convert_list: text.i18n_convert_list,
            bgm_list: text.bgm_list,
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
            i18n_convert_list: text.i18n_convert_list,
            bgm_list: text.bgm_list,
        }
    }

    /// BGM 名が一致したものを返す。そうでない場合は推測して推測率と返す
    pub fn convert_bgm_list(maybe_bgm_name: String) -> Option<(String, f32)> {
        if let Some(bgm_name) = SMASHBROS_RESOURCE().get_mut().bgm_list.iter().find(|&c| c.0 == &maybe_bgm_name) {
            // 完全一致(公式名)
            return Some(( bgm_name.0.clone(), 1.0 ));
        } else {
            // 完全一致(公式名)から一番一致率が高い名前を設定する
            let bgm_list = SMASHBROS_RESOURCE().get_mut().bgm_list.clone().into_keys().collect::<Vec<String>>();
            let (bgm_name, ratio) = Self::matcher(&bgm_list, &maybe_bgm_name, None);
            if !bgm_name.is_empty() {
                return Some(( bgm_name, ratio ));
            }
        }

        None
    }

    /// キャラ名が一致したものを返す。そうでない場合は推測して推測率と返す
    pub fn convert_character_name(maybe_character_name: String) -> Option<(String, f32)> {
        if SMASHBROS_RESOURCE().get_mut().character_list.contains_key(&maybe_character_name) {
            // 完全一致(公式英名)
            return Some(( maybe_character_name.clone(), 1.0 ));
        } else if let Some(chara_name) = SMASHBROS_RESOURCE().get_mut().character_list.iter().find(|&c| c.1 == &maybe_character_name) {
            // 完全一致(公式名)
            return Some(( chara_name.0.clone(), 1.0 ));
        } else if let Some(chara_name) = SMASHBROS_RESOURCE().get_mut().i18n_convert_list.get(&maybe_character_name) {
            // i18n(各言語名)
            return Some(( chara_name.clone(), 1.0 ));
        } else {
            // 完全一致(公式英名)から一番一致率が高い名前を設定する
            let chara_list = SMASHBROS_RESOURCE().get_mut().character_list.clone().into_keys().collect::<Vec<String>>();
            let (chara_name, ratio) = Self::matcher(&chara_list, &maybe_character_name, Some(SmashbrosData::CHARACTER_NAME_UNKNOWN));
            if chara_name != SmashbrosData::CHARACTER_NAME_UNKNOWN {
                return Some(( chara_name, ratio ));
            }

            // 完全一致(公式名)から一番一致率が高い名前を設定する
            let chara_list = SMASHBROS_RESOURCE().get_mut().character_list.clone().into_values().collect::<Vec<String>>();
            let (chara_name, ratio) = Self::matcher(&chara_list, &maybe_character_name, Some(SmashbrosData::CHARACTER_NAME_UNKNOWN));
            if chara_name != SmashbrosData::CHARACTER_NAME_UNKNOWN {
                if let Some(( chara_name, _)) = SMASHBROS_RESOURCE().get_mut().character_list.iter().find(|&c| c.1 == &chara_name) {
                    return Some(( chara_name.clone(), ratio ));
                }
            }
        }

        None
    }

    pub fn load_texture_handle(name: &str, ctx: &egui::Context, image: &opencv::core::Mat) -> TextureHandle {
        let image_size = ( image.cols() * image.rows() * 4 ) as usize;
        let image_data_by_slice: &[u8] = unsafe{ std::slice::from_raw_parts(image.datastart(), image_size) };
        
        ctx.load_texture(
            name,
            egui::ColorImage::from_rgba_unmultiplied(
                [image.cols() as usize, image.rows() as usize],
                image_data_by_slice,
            )
        )
    }

    pub fn get_image_handle(&self, character_name: String) -> Option<TextureHandle> {
        if !self.icon_list.contains_key(&character_name) {
            return None;
        }

        Some(self.icon_list[&character_name].clone())
    }

    pub fn get_order_handle(&self, order: i32) -> Option<TextureHandle> {
        if order <= 0 || 5 <= order {
            return None;
        }

        Some(self.order_image_list[&order].clone())
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
    pub fn init(&mut self, ctx: Option<&egui::Context>) {
        if self.smashbros_resource.is_none() {
            self.smashbros_resource = Some(SmashbrosResource::new(ctx.unwrap()));
        }
    }

    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    pub fn get_mut(&mut self) -> &mut SmashbrosResource {
        if self.smashbros_resource.is_none() {
            log::error!("SmashbrosResource is not initialized. Call init() first.");
            self.smashbros_resource = Some(SmashbrosResource::new_for_test());
        }
        self.smashbros_resource.as_mut().unwrap()
    }
}
static mut _SMASHBROS_RESOURCE: WrappedSmashbrosResource = WrappedSmashbrosResource {
    smashbros_resource: None,
};
#[allow(non_snake_case)]
pub fn SMASHBROS_RESOURCE() -> &'static mut WrappedSmashbrosResource {
    unsafe { &mut _SMASHBROS_RESOURCE }
}

// GUI の状態を保持するためのデータ
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GUIStateConfig {
    #[serde(default = "GUIStateConfig::default_chara_image")]
    pub chara_image: bool,
    #[serde(default = "GUIStateConfig::default_win_rate")]
    pub win_rate: bool,
    #[serde(default = "GUIStateConfig::default_win_lose")]
    pub win_lose: bool,
    #[serde(default = "GUIStateConfig::default_wins")]
    pub wins: bool,
    #[serde(default = "GUIStateConfig::default_graph")]
    pub graph: bool,
    #[serde(default = "GUIStateConfig::default_gsp")]
    pub gsp: bool,
    #[serde(default = "GUIStateConfig::default_battling")]
    pub battling: bool,
    #[serde(default = "GUIStateConfig::default_show_captured")]
    pub show_captured: bool,
    #[serde(default = "GUIStateConfig::default_disable_volume")]
    pub disable_volume: f32,
    #[serde(default = "GUIStateConfig::default_play_list_volume")]
    pub play_list_volume: f32,
    #[serde(default = "GUIStateConfig::default_stock_warning_under")]
    pub stock_warning_under: i32,
}
impl Default for GUIStateConfig {
    fn default() -> Self {
        Self {
            chara_image: Self::default_chara_image(),
            win_rate: Self::default_win_rate(),
            win_lose: Self::default_win_lose(),
            wins: Self::default_wins(),
            graph: Self::default_graph(),
            gsp: Self::default_gsp(),
            battling: Self::default_battling(),
            show_captured: Self::default_show_captured(),
            disable_volume: Self::default_disable_volume(),
            play_list_volume: Self::default_play_list_volume(),
            stock_warning_under: Self::default_stock_warning_under(),
        }
    }
}
impl GUIStateConfig {
    pub fn is_show_wins_group(&self) -> bool {
        self.chara_image || self.win_rate || self.win_lose || self.wins 
    }

    pub fn default_chara_image() -> bool { true }
    pub fn default_win_rate() -> bool { true }
    pub fn default_win_lose() -> bool { true }
    pub fn default_wins() -> bool { true }
    pub fn default_graph() -> bool { true }
    pub fn default_gsp() -> bool { false }
    pub fn default_battling() -> bool { true }
    pub fn default_show_captured() -> bool { true }
    pub fn default_disable_volume() -> f32 { 0.0 }
    pub fn default_play_list_volume() -> f32 { 1.0 }
    pub fn default_stock_warning_under() -> i32 { 3 }
}


// 設定ファイル
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct GUIConfig {
    pub window_x: i32,
    pub window_y: i32,

    // バージョン更新でファイルに値が無い場合があるので、以下から default を追加する
    #[serde(default)]
    pub capture_mode: crate::capture::CaptureMode,
    #[serde(default)]
    pub visuals: Option<egui::style::Visuals>,
    #[serde(deserialize_with = "GUIConfig::deserialized_lang", serialize_with = "GUIConfig::serialize_lang")]
    pub lang: Option<LanguageIdentifier>,
    #[serde(default = "crate::engine::SmashBrogEngine::get_default_result_limit")]
    pub result_max: i64,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_size: Option<i32>,
    #[serde(default)]
    pub bgm_device_name: Option<String>,
    #[serde(default)]
    pub bgm_session_name: Option<String>,
    #[serde(default = "GUIConfig::default_bgm_playlist_folder")]
    pub bgm_playlist_folder: String,
    #[serde(default = "GUIConfig::default_stock_alert_command")]
    pub stock_alert_command: String,
    #[serde(default)]
    pub gui_state_config: GUIStateConfig,
}
impl GUIConfig {
    const DEFAULT_CAPTION: &'static str = "smabrog";
    const CONFIG_FILE: &'static str = "config.json";

    /// 設定情報の読み込み
    pub fn load_config(&mut self, is_initalize: bool) -> anyhow::Result<()> {
        let file = std::fs::File::open(Self::CONFIG_FILE)?;
        match serde_json::from_reader(std::io::BufReader::new(file)) {
            Ok(own) => *self = own,
            Err(e) => {
                log::warn!("skip load config for version up [<=31] -> [32<=]: {}", e);

                self.result_max = crate::engine::SmashBrogEngine::get_default_result_limit();
                self.bgm_playlist_folder = GUIConfig::default_bgm_playlist_folder();
                self.stock_alert_command = GUIConfig::default_stock_alert_command();
            }
        }

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

    // LanguageIdentifier の変換用
    pub fn deserialized_lang<'de, D>(deserializer: D) -> Result<Option<LanguageIdentifier>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let lang_str = String::deserialize(deserializer)?;
        if lang_str.is_empty() {
            return Ok(Some( LanguageIdentifier::from_bytes("ja-JP".as_bytes()).expect("lang parsing failed") ));
        }
        Ok(Some( LanguageIdentifier::from_bytes(lang_str.as_bytes()).unwrap() ))
    }
    pub fn serialize_lang<S>(lang: &Option<LanguageIdentifier>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(lang) = lang {
            serializer.serialize_str(lang.to_string().as_str())
        } else {
            serializer.serialize_str("ja-JP")
        }
    }
    
    pub fn default_bgm_playlist_folder() -> String { "./playlist".to_string() }
    pub fn default_stock_alert_command() -> String { "./resource/danger.avi".to_string() }
}
/// シングルトンで設定ファイルを保持するため
pub struct WrappedGUIConfig {
    gui_config: Option<GUIConfig>
}
impl WrappedGUIConfig {
    // 参照して返さないと、unwrap() で move 違反がおきてちぬ！
    pub fn get_mut(&mut self) -> &mut GUIConfig {
        if self.gui_config.is_none() {
            self.gui_config = Some(GUIConfig::default());
        }
        self.gui_config.as_mut().unwrap()
    }
}
static mut _GUI_CONFIG: WrappedGUIConfig = WrappedGUIConfig {
    gui_config: None,
};
#[allow(non_snake_case)]
pub fn GUI_CONFIG() -> &'static mut WrappedGUIConfig {
    unsafe { &mut _GUI_CONFIG }
}


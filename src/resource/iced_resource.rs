
use serde::{
    Deserialize,
    Serialize,
[]};
use std::collections::HashMap;
use std::io::BufReader;


/// 画像とかも入ったリソース
pub struct SmashbrosResource {
    pub version: String,
    pub character_list: HashMap<String, String>,
    pub icon_list: HashMap<String, iced_winit::image::Handle>,
    pub order_image_list: HashMap<i32, iced_winit::image::Handle>,
}
impl SmashbrosResource {
    fn new() -> Self {
        let text = SmashbrosResourceText::new();
        let mut icon_list: HashMap<String, iced_winit::image::Handle> = HashMap::new();
        for (character_name, file_name) in text.icon_list.iter() {
            icon_list.insert(
                character_name.to_string(),
                iced_winit::image::Handle::from_path(format!( "icon/{}", file_name ))
            );
            
        }
        let mut order_image_list: HashMap<i32, iced_winit::image::Handle> = HashMap::new();
        for order in 0..4 {
            order_image_list.insert(
                order,
                iced_winit::image::Handle::from_path(format!( "resource/result_player_order_{}_color.png", order ))
            );
        }

        Self {
            version: text.version,
            character_list: text.character_list,
            icon_list: icon_list,
            order_image_list: order_image_list,
        }
    }

    pub fn get_image_handle(&self, character_name: String) -> Option<iced_winit::image::Handle> {
        if !self.icon_list.contains_key(&character_name) {
            return None;
        }

        Some(self.icon_list[&character_name].clone())
    }

    pub fn get_order_handle(&self, order: i32) -> Option<iced_winit::image::Handle> {
        if order <= 0 || 5 <= order {
            return None;
        }

        Some(self.order_image_list[&order].clone())
    }
}

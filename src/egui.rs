
use eframe::{
    epi,
    egui::{
        self,
        plot,
    },
};
use opencv::prelude::MatTrait;

use crate::capture::CaptureMode;
use crate::data::{
    SmashbrosData,
    SmashbrosDataTrait,
};
use crate::engine::SmashBrogEngine;
use crate::resource::{
    GUI_CONFIG,
    SMASHBROS_RESOURCE,
};
use crate::scene::SceneList;


pub fn make_gui_run() -> anyhow::Result<()> {
    let mut native_options = eframe::NativeOptions::default();
    native_options.icon_data = Some(GUI::get_icon_data());
    native_options.initial_window_size = Some(GUI::get_initial_window_size());

    let app = GUI::new();
    eframe::run_native(Box::new(app), native_options);

    Ok(())
}


// GUIの種類, is_source に指定するのに必要
#[derive(std::hash::Hash)]
enum GUIIdList {
    SourceKind,
    WindowList,
    DeviceList,
    BattleInformationGrid,
    PowerPlot,
}

// GUI の子ウィンドウが持つ
trait GUIModelTrait {
    fn name(&self) -> &'static str;
    fn show(&mut self, ctx: &egui::CtxRef);
    fn setup(&mut self, ctx: &egui::CtxRef);
}
trait  GUIViewTrait {
    fn ui(&mut self, ui: &mut egui::Ui);
}


pub struct GUI {
    engine: SmashBrogEngine,
    capture_mode: CaptureMode,
    window_battle_information: WindowBattleInformation,
    window_battle_history: WindowBattleHistory,
    window_configuration: WindowConfiguration,
}
impl GUI {
    fn new() -> Self {
        Self {
            engine: SmashBrogEngine::default(),
            capture_mode: CaptureMode::default(),
            window_battle_information: WindowBattleInformation::default(),
            window_battle_history: WindowBattleHistory::default(),
            window_configuration: WindowConfiguration::default(),
        }
    }

    pub fn get_icon_data() -> epi::IconData {
        let window_icon = opencv::imgcodecs::imread("icon/smabrog.png", opencv::imgcodecs::IMREAD_UNCHANGED).unwrap();
        let icon_size = ( window_icon.cols() * window_icon.rows() * 4 ) as usize;
        let icon_data_by_slice: &[u8] = unsafe{ std::slice::from_raw_parts(window_icon.datastart(), icon_size) };
    
        epi::IconData {
            rgba: icon_data_by_slice.to_vec(),
            width: window_icon.cols() as u32,
            height: window_icon.rows() as u32,
        }
    }

    pub fn get_chara_image<D: SmashbrosDataTrait + ?Sized>(data: &D, player_id: i32, size: [f32; 2]) -> Option<egui::Image> {
        if let Some(chara_texture) = unsafe{ SMASHBROS_RESOURCE.get() }.get_image_handle(data.get_character(player_id)) {
            return Some(egui::Image::new(chara_texture, egui::Vec2::new(size[0], size[1])));
        }

        None
    }

    pub fn get_initial_window_size() -> egui::Vec2 { egui::Vec2::new(256f32, 720f32) }

    pub fn get_title_bar_height() -> f32 { 32.0 }

    pub fn set_fonts(&self, ctx: &egui::CtxRef) {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "Mamelon".to_string(),
            std::borrow::Cow::Borrowed(include_bytes!("../fonts/Mamelon-5-Hi-Regular.otf"))
        );
        fonts.fonts_for_family
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "Mamelon".to_string());

        fonts.family_and_size.insert(egui::TextStyle::Heading, (egui::FontFamily::Proportional, 14.0));
        fonts.family_and_size.insert(egui::TextStyle::Button, (egui::FontFamily::Proportional, 12.0));

        ctx.set_fonts(fonts);
    }

    // 対戦情報の更新
    pub fn update_battle_informations(&mut self) {
        // 対戦中情報
        self.window_battle_information.battle_information.set_data(Box::new( self.engine.get_now_data() ));

        // 戦歴
        self.window_battle_history.battle_information_list.clear();
        for data in self.engine.get_data_latest_10() {
            let mut battle_information = WindowBattleInformationGroup::default();
            battle_information.set_data( Box::new(data) );

            self.window_battle_history.battle_information_list.push(battle_information);
        }

        let data_list = self.engine.get_data_latest_500_by_now_chara();
        self.window_battle_information.wins_graph.set_data(
            self.engine.get_now_data(),
            self.engine.get_data_latest_10(),
            self.engine.get_win_lose_latest_10(),
            self.engine.get_wins_by_data_list(data_list)
        );

        // 検出状態
        self.window_configuration.now_scene = self.engine.get_captured_scene();
    }

    // 検出モードの変更
    fn set_capture_mode(&mut self) {
        if self.window_configuration.get_captured_mode() != &self.capture_mode {
            self.capture_mode = self.window_configuration.get_captured_mode().clone();
            if self.capture_mode.is_default() {
                // 未選択状態での設定はコンフィグから取得しておく
                match self.capture_mode.as_mut() {
                    CaptureMode::Window(_, caption_name) => {
                        *caption_name = unsafe{ GUI_CONFIG.get() }.capture_win_caption.clone();
                    },
                    CaptureMode::VideoDevice(_, device_id, _) => {
                        *device_id = self.window_configuration.get_device_id(
                            unsafe{ GUI_CONFIG.get() }.capture_device_name.clone()
                        ).unwrap_or(-1);
                    },
                    _ => (),
                }
            }

            self.window_configuration.set_capture_mode(self.capture_mode.clone());

            match self.engine.change_capture_mode(&self.capture_mode) {
                Ok(_) => {
                    unsafe { GUI_CONFIG.get().save_config(false); }
                },
                Err(e) => log::warn!("{}", e),
            }
        }
    }

    // 幅が 0 の egui::Grid を返す
    fn new_grid<T>(id_source: T, columns: usize, spacing: egui::Vec2) -> egui::Grid where T: std::hash::Hash {
        egui::Grid::new(id_source)
            .num_columns(columns)
            .spacing(spacing)
            .min_col_width(0.0)
            .min_row_height(0.0)
    }
}
impl epi::App for GUI {
    fn name(&self) -> &str { "smabrog" }

    fn setup(&mut self, ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>, _storage: Option<&dyn epi::Storage>) {
        self.set_fonts(ctx);
        unsafe {
            SMASHBROS_RESOURCE.init(Some(frame));
            GUI_CONFIG.get().load_config(true);
        }

        self.window_battle_information.setup(ctx);
        self.window_battle_history.setup(ctx);
        self.window_configuration.setup(ctx);

        self.window_battle_information.battle_information = WindowBattleInformationGroup::default();
        self.update_battle_informations();
    }

    fn on_exit(&mut self) {
        unsafe {
            GUI_CONFIG.get().save_config(true);
        }
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>) {
        /* 子ウィンドウを3つ作成する
         * [対戦中情報]
         *   .対戦情報グループ(リアルタイム更新)
         *   .直近勝率(10, 50件)
         *   .戦闘力(1万以下切り捨て表示)
         * [戦歴]
         *   .対戦情報グループ(過去 10 件分)
         * [設定]
         *   .ソースの設定
         *     .ウィンドウから
         *     .ビデオデバイスから
         *     .デスクトップから
         *     .未設定
         * 
         * .対戦情報グループ
         *   .1pキャラアイコン vs 2pキャラアイコン
         *   .ルール(アイコンにしたい), 時間
         *   .ストック(アイコンにしたい)
         */ 

        // 動作
        if let Err(e) = self.engine.update() {
            // quit
            // ゆくゆくはエラー回復とかもできるようにしたい
            log::error!("quit. [{}]", e);
            frame.quit();
            return;
        }
        self.update_battle_informations();
        self.set_capture_mode();

        // 表示
        self.window_battle_information.show(ctx);
        self.window_battle_history.show(ctx);
        self.window_configuration.show(ctx);

        // frame.repaint_signal();
        ctx.request_repaint();
    }
}

// 対戦中情報
#[derive(Default)]
struct WindowBattleInformation {
    pub battle_information: WindowBattleInformationGroup,
    pub wins_graph: WindowWinsGraph,
}
impl WindowBattleInformation {
    pub fn get_initial_window_size() -> egui::Vec2 {
        let parent_size = GUI::get_initial_window_size();

        egui::Vec2::new(parent_size.x, parent_size.y/10.0*8.0 / 10.0 * 2.0 - GUI::get_title_bar_height())
    }

    pub fn get_initial_window_rect() -> egui::Rect {
        egui::Rect::from_min_size(
            egui::Pos2::new(0.0, 0.0),
            Self::get_initial_window_size(),
        )
    }
}
impl GUIModelTrait for WindowBattleInformation {
    fn name(&self) -> &'static str { "対戦情報" }
    fn show(&mut self, ctx: &egui::CtxRef) {
        egui::Window::new(self.name())
            .default_rect(Self::get_initial_window_rect())
            .show(ctx, |ui| self.ui(ui));
    }
    fn setup(&mut self, ctx: &egui::CtxRef) {
    }
}
impl GUIViewTrait for WindowBattleInformation {
    fn ui(&mut self, ui: &mut egui::Ui) {
        self.battle_information.show_ui(ui);
        ui.separator();
        self.wins_graph.show_ui(ui);

        ui.allocate_space(ui.available_size());
    }
}

// 戦歴
#[derive(Default)]
struct WindowBattleHistory {
    pub battle_information_list: Vec<WindowBattleInformationGroup>,
}
impl WindowBattleHistory {
    pub fn get_initial_window_size() -> egui::Vec2 {
        let parent_size = GUI::get_initial_window_size();

        egui::Vec2::new(parent_size.x, parent_size.y/10.0*8.0 / 10.0 * 8.0 - GUI::get_title_bar_height())
    }

    pub fn get_initial_window_rect() -> egui::Rect {
        egui::Rect::from_min_size(
            egui::Pos2::new(0.0, WindowBattleInformation::get_initial_window_size().y + GUI::get_title_bar_height()),
            Self::get_initial_window_size(),
        )
    }
}
impl GUIModelTrait for WindowBattleHistory {
    fn name(&self) -> &'static str { "戦歴" }
    fn show(&mut self, ctx: &egui::CtxRef) {
        egui::Window::new(self.name())
            .default_rect(Self::get_initial_window_rect())
            .vscroll(true)
            .show(ctx, |ui| self.ui(ui));
    }
    fn setup(&mut self, ctx: &egui::CtxRef) {
    }
}
impl GUIViewTrait for WindowBattleHistory {
    fn ui(&mut self, ui: &mut egui::Ui) {
        for group in &mut self.battle_information_list {
            group.show_ui(ui);
            ui.separator();
        }
        
        ui.allocate_space(ui.available_size());
    }
}

// 設定
#[derive(Default)]
struct WindowConfiguration {
    pub now_scene: SceneList,
    capture_mode: CaptureMode,
    window_caption: String,
    video_device_id: i32,
    video_device_list: Vec<String>,
    window_caption_list: Vec<String>,
}
impl WindowConfiguration {
    pub fn get_initial_window_size() -> egui::Vec2 {
        let parent_size = GUI::get_initial_window_size();

        egui::Vec2::new(parent_size.x, parent_size.y / 10.0 * 2.0 - GUI::get_title_bar_height())
    }

    pub fn get_initial_window_rect() -> egui::Rect {
        egui::Rect::from_min_size(
            egui::Pos2::new(0.0, GUI::get_initial_window_size().y - Self::get_initial_window_size().y),
            Self::get_initial_window_size(),
        )
    }

    pub fn set_capture_mode(&mut self, mode: CaptureMode) {
        self.capture_mode = mode;
    }

    pub fn get_captured_mode(&self) -> &CaptureMode {
        &self.capture_mode
    }

    pub fn get_device_id(&self, device_name: String) -> Option<i32> {
        if let Some(id) = self.video_device_list.iter().position(|name| name == &device_name) {
            Some(id as i32)
        } else {
            None
        }
    }

    fn source_settings_view(&mut self, ui: &mut egui::Ui) {
        use crate::capture::{
            CaptureFromWindow,
            CaptureFromVideoDevice,
        };

        egui::ComboBox::from_id_source(GUIIdList::SourceKind)
            .width(ui.available_size().x - 10.0)
            .selected_text(format!("{}", self.capture_mode))
            .show_ui(ui, |ui| {
                if ui.add(egui::SelectableLabel::new(self.capture_mode.is_empty(), "未設定")).clicked() {
                    self.capture_mode = CaptureMode::new_empty();
                }
                if ui.add(egui::SelectableLabel::new(self.capture_mode.is_window(), "ウィンドウ".to_string())).clicked() {
                    self.capture_mode = CaptureMode::new_window(self.window_caption.clone());
                    self.window_caption_list = CaptureFromWindow::get_window_list();
                }
                if ui.add(egui::SelectableLabel::new(self.capture_mode.is_video_device(), "ビデオデバイス".to_string())).clicked() {
                    self.capture_mode = CaptureMode::new_video_device(self.video_device_id);
                    self.video_device_list = CaptureFromVideoDevice::get_device_list();
                }
                if ui.add(egui::SelectableLabel::new(self.capture_mode.is_desktop(), "デスクトップ".to_string())).clicked() {
                    self.capture_mode = CaptureMode::new_desktop();
                }
            });
        ui.end_row();

        let Self {
            video_device_list,
            window_caption_list,
            ..
        } = self;
        match &mut self.capture_mode {
            CaptureMode::Window(_, window_caption) => {
                // 長すぎると表示が崩れるので短くする(UTF-8だと境界がおかしいと None になるっぽいので 4byte分見る)
                let selected_text = window_caption.clone();
                let l = std::cmp::min(selected_text.len(), 25);
                let mut selected_text = selected_text.get(0..l).unwrap_or(
                    selected_text.get(0..(l+1)).unwrap_or(
                        selected_text.get(0..(l+2)).unwrap_or(
                            selected_text.get(0..(l+3)).unwrap_or("")
                        )
                    )
                ).to_string();
                selected_text += if selected_text.len() == window_caption.len() { "" } else { "..." };
                
                egui::ComboBox::from_id_source(GUIIdList::WindowList)
                    .selected_text(selected_text)
                    .width(ui.available_size().x - 10.0)
                    .show_ui(ui, |ui| {
                        for wc in window_caption_list {
                            ui.selectable_value(window_caption, wc.clone(), wc.clone());
                        }
                    });
            },
            CaptureMode::VideoDevice(_, device_id, _) => {
                let video_device_list = CaptureFromVideoDevice::get_device_list();

                egui::ComboBox::from_id_source(GUIIdList::DeviceList)
                    .selected_text(format!("{}", video_device_list.get(*device_id as usize).unwrap_or(&"未選択".to_string())))
                    .width(ui.available_size().x - 10.0)
                    .show_ui(ui, |ui| {
                        for (id, name) in video_device_list.iter().enumerate() {
                            if ui.add(egui::SelectableLabel::new(*device_id == id as i32, name)).clicked() {
                                *device_id = id as i32;
                            }
                        }
                    });
            },
            _ => (),
        }
        ui.end_row();
    }
}
impl GUIModelTrait for WindowConfiguration {
    fn name(&self) -> &'static str { "状態" }
    fn show(&mut self, ctx: &egui::CtxRef) {
        egui::Window::new( format!("{}: {:?}", self.name(), self.now_scene) )
            .default_rect(Self::get_initial_window_rect())
            .show(ctx, |ui| self.ui(ui));
    }
    fn setup(&mut self, ctx: &egui::CtxRef) {
        self.video_device_id = -1;
    }
}
impl GUIViewTrait for WindowConfiguration {
    fn ui(&mut self, ui: &mut egui::Ui) {
        self.source_settings_view(ui);
        ui.allocate_space(ui.available_size());
    }
}

// 対戦情報グループ
#[derive(Default)]
struct WindowBattleInformationGroup {
    data: Option<Box<dyn SmashbrosDataTrait>>,
}
impl WindowBattleInformationGroup {
    fn set_data(&mut self, data: Box<dyn SmashbrosDataTrait>) {
        self.data = Some(data);
    }

    // キャラと順位の表示
    fn show_player_chara(&self, ui: &mut egui::Ui, data: &Box<dyn SmashbrosDataTrait>, player_id: i32) {
        if let Some(chara_image) = GUI::get_chara_image(data.as_ref(), player_id, [32.0, 32.0]) {
            ui.add_sized( [32.0, 32.0], chara_image);
        } else {
            ui.add_sized( [32.0, 32.0], egui::Label::new(format!("{}p", player_id + 1)) );
        }

        egui::Grid::new(GUIIdList::BattleInformationGrid)
            .num_columns(2)
            .spacing(egui::Vec2::new(0.0, 0.0))
            .min_col_width(0.0)
            .min_row_height(16.0)
            .show(ui, |ui| {
                ui.end_row();
                if let Some(order_texture) = unsafe{ SMASHBROS_RESOURCE.get() }.get_order_handle(data.get_order(player_id)) {
                    let size = unsafe{ SMASHBROS_RESOURCE.get() }.get_image_size(order_texture).unwrap();
                    ui.add_sized( [10.0, 16.0], egui::Image::new(order_texture, size * egui::Vec2::new(0.25, 0.25)) );
                } else {
                    ui.add_sized( [10.0, 16.0], egui::Label::new("?") );
                }
            });
    }

    // ストックの表示 (3 ストック以下ならアイコン表示、それ以上ならアイコンと数値を表示)
    fn show_player_stock(&self, ui: &mut egui::Ui, data: &Box<dyn SmashbrosDataTrait>, player_id: i32) {
        let stock = data.get_stock(player_id);
        for i in 0..3 {
            if (0 != stock) && (i < stock || 0 == i) {
                if let Some(chara_image) = GUI::get_chara_image(data.as_ref(), player_id, [16.0, 16.0]) {
                    ui.add_sized( [16.0, 16.0], chara_image);
                } else {
                    ui.add_sized( [16.0, 16.0], egui::Label::new("?"));
                }
            } else if 4 <= stock && 1 == i {
                // 4 以上のストックを表示
                ui.add_sized( [16.0, 16.0], egui::Label::new(format!("{}", stock)) );
            } else {
                // 空で行を詰める
                ui.add_sized( [16.0, 16.0], egui::Label::new("") );
            }
        }
    }

    fn show_ui(&self, ui: &mut egui::Ui) {
        /*
         * [対戦情報グループ]
         * .1pキャラアイコン vs 2pキャラアイコン
         * .ルール(アイコンにしたい), 時間
         * .ストック(アイコンにしたい)
         */
        let data = match self.data.as_ref() {
            Some(data) => {
                if data.get_player_count() == 4 {
                    // 4 人は未対応
                    return;
                }

                data
            },
            None => {
                // データなしを表示
                return;
            },
        };

        ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, 0.0);
        GUI::new_grid(GUIIdList::BattleInformationGrid, 2, egui::Vec2::new(0.0, 0.0))
            .show(ui, |ui| {
                // [ham vs spam] の表示
                GUI::new_grid("character_icons", 3, egui::Vec2::new(5.0, 0.0))
                    .show(ui, |ui| {
                        self.show_player_chara(ui, data, 0);
                        ui.add_sized( [16.0, 16.0], egui::Label::new("vs") );
                        self.show_player_chara(ui, data, 1);
                        ui.end_row();
                    });

                ui.add(egui::Separator::default().vertical());

                // 最大ストック,制限時間 の表示
                GUI::new_grid("rules_icons", 2, egui::Vec2::new(0.0, 0.0))
                    .show(ui, |ui| {
                        let max_stock = data.get_max_stock(0);
                        ui.add_sized( [16.0, 16.0], egui::Label::new("👥") );
                        ui.add_sized( [16.0, 16.0], egui::Label::new(format!(
                            "{}",
                            if max_stock == -1 { "?".to_string() } else { max_stock.to_string() }
                        )));

                        ui.end_row();

                        let max_time = data.get_max_time().as_secs() / 60;
                        ui.add_sized( [16.0, 16.0], egui::Label::new("⏱") );
                        ui.add_sized( [16.0, 16.0], egui::Label::new(format!(
                            "{}",
                            if max_time == 0 { "?".to_string() } else { max_time.to_string() }
                        )));
                    });

                ui.add(egui::Separator::default().vertical());

                // ストックの表示
                GUI::new_grid("stocks_icons", 3, egui::Vec2::new(0.0, 0.0))
                    .show(ui, |ui| {
                        self.show_player_stock(ui, data, 0);
                        ui.end_row();
                        self.show_player_stock(ui, data, 1);
                    });

                ui.end_row();
            });
    }
}

// 勝率および戦闘力グループ
#[derive(Default)]
struct WindowWinsGraph {
    now_data: Option<SmashbrosData>,
    point_list: Vec<plot::Value>,
    last_power: i32,
    wins_lose: (i32, i32),
    win_rate: f32,
}
impl WindowWinsGraph {
    fn set_data(&mut self, data: SmashbrosData, data_list: Vec<SmashbrosData>, wins_lose: (i32, i32), win_rate: f32) {
        let mut data_list = data_list;
        data_list.reverse();
        self.point_list = data_list.iter().enumerate().map(|(x, data)| {
            plot::Value::new(x as f64, data.get_power(0) as f64)
        }).collect::<Vec<plot::Value>>();

        self.now_data = Some(data);
        self.last_power = data_list.last().unwrap().get_power(0);
        self.wins_lose = wins_lose;
        self.win_rate = win_rate;
    }

    fn show_ui(&self, ui: &mut egui::Ui) {
        let points_values = plot::Values::from_values(self.point_list.clone());
        let line_values = plot::Values::from_values(self.point_list.clone());
        let gcp_plot = plot::Plot::new(GUIIdList::PowerPlot)
            .width(GUI::get_initial_window_size().x / 2.0)
            .height(40.0)
            .legend(plot::Legend::default())
            .view_aspect(1.0)
            .show_axes([false, false])
            .line(plot::Line::new(line_values).color(egui::Color32::WHITE))
            .points(
                plot::Points::new(points_values).radius(2.0).color(egui::Color32::RED)
                    .name(format!("世界戦闘力\n{}", self.last_power))
            );

        GUI::new_grid("wins_graph_group", 2, egui::Vec2::new(0.0, 0.0))
            .min_col_width(120.0)
            .show(ui, |ui| {
                GUI::new_grid("wins_group", 6, egui::Vec2::new(0.0, 0.0))
                    .show(ui, |ui| {
                        // 勝率
                        if let Some(now_data) = &self.now_data {
                            if now_data.is_decided_character_name(0) && now_data.is_decided_character_name(1) {
                                ui.add_sized( [16.0, 16.0], GUI::get_chara_image(now_data.as_ref(), 0, [16.0, 16.0]).unwrap());
                                ui.add_sized( [16.0, 16.0], egui::Label::new("vs"));
                                ui.add_sized( [16.0, 16.0], GUI::get_chara_image(now_data.as_ref(), 1, [16.0, 16.0]).unwrap());
                                ui.add_sized( [16.0, 16.0], egui::Label::new(format!("{:3.1}%", 100.0 * self.win_rate)));
                            }
                        }
                        ui.end_row();

                        // 対キャラクター勝率
                    });
                
                ui.add(gcp_plot);
            });
    }
}

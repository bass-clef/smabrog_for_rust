
use eframe::{
    epi,
    egui::{
        self,
        plot,
        style::Visuals,
    },
};
use i18n_embed_fl::fl;
use linked_hash_map::LinkedHashMap;
use opencv::prelude::MatTraitConst;
use std::collections::HashMap;

use crate::capture::CaptureMode;
use crate::data::{
    SmashbrosData,
    SmashbrosDataTrait,
};
use crate::engine::SmashBrogEngine;
use crate::resource::{
    gui_config,
    smashbros_resource,
    lang_loader,
};
use crate::scene::SceneList;


pub fn make_gui_run() -> anyhow::Result<()> {
    let mut native_options = eframe::NativeOptions::default();
    native_options.icon_data = Some(GUI::get_icon_data());
    native_options.initial_window_size = Some(GUI::get_initial_window_size());
    native_options.resizable = false;

    let app = GUI::new();

    eframe::run_native(Box::new(app), native_options)
}


// GUIの種類, is_source に指定するのに必要
#[derive(std::hash::Hash)]
enum GUIIdList {
    AppearanceTab,
    SourceKind,
    LanguageKind,

    WindowList,
    DeviceList,

    BattleInformationGrid,
    PowerPlot,
    CharacterPlot,
}

// GUI の子ウィンドウが持つ
trait GUIModelTrait {
    fn name(&self) -> String;
    fn show(&mut self, ctx: &egui::CtxRef);
    fn setup(&mut self, _ctx: &egui::CtxRef) {}
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

    // GUI の icon を返す
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

    // data の player_id のキャラ画像を指定 size で返す
    pub fn get_chara_image(chara_name: String, size: [f32; 2]) -> Option<egui::Image> {
        if let Some(chara_texture) = smashbros_resource().get().get_image_handle(chara_name) {
            return Some(egui::Image::new(chara_texture, egui::Vec2::new(size[0], size[1])));
        }

        None
    }

    // 初期化ウィンドウサイズを返す
    pub fn get_initial_window_size() -> egui::Vec2 { egui::Vec2::new(256f32, 720f32) }

    // タイトルバーの高さを返す
    pub fn get_title_bar_height() -> f32 { 32.0 }

    // フォントの設定
    pub fn set_fonts(&self, ctx: &egui::CtxRef) {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "Mamelon".to_string(),
            egui::FontData::from_static(include_bytes!("../fonts/Mamelon-5-Hi-Regular.otf"))
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
        if !self.engine.update_now_data() {
            return;
        }

        // 対戦中情報
        self.window_battle_information.battle_information.set_data( self.engine.get_now_data() );

        // 戦歴
        self.window_battle_history.battle_information_list.clear();
        for data in self.engine.get_data_latest_10() {
            let mut battle_information = WindowBattleInformationGroup::default();
            battle_information.set_data(data);

            self.window_battle_history.battle_information_list.push(battle_information);
        }
        let data_list = self.engine.get_data_all_by_now_chara();
        self.window_battle_history.set_data(self.engine.get_wins_by_data_list_groupby_character(data_list));

        let data_list = self.engine.get_data_latest_by_now_chara();
        self.window_battle_information.wins_graph.set_data(
            self.engine.get_now_data(),
            self.engine.get_data_latest_10(),
            self.engine.get_win_lose_latest_10(),
            self.engine.get_wins_by_data_list(data_list),
        );

        // 検出状態
        self.window_configuration.now_scene = self.engine.get_captured_scene();
        self.window_configuration.prev_match_ratio = self.engine.get_prev_match_ratio();
    }

    // 検出モードの更新
    fn update_capture_mode(&mut self) {
        if self.window_configuration.get_captured_mode() == &self.capture_mode {
            return;
        }

        self.capture_mode = self.window_configuration.get_captured_mode().clone();
        if self.capture_mode.is_default() {
            // 未選択状態での設定はコンフィグから取得しておく
            match self.capture_mode.as_mut() {
                CaptureMode::Window(_, caption_name) => {
                    *caption_name = gui_config().get().capture_win_caption.clone();
                },
                CaptureMode::VideoDevice(_, device_id, _) => {
                    *device_id = self.window_configuration.get_device_id(
                        gui_config().get().capture_device_name.clone()
                    ).unwrap_or(-1);
                },
                _ => (),
            }
        }

        self.window_configuration.set_capture_mode(self.capture_mode.clone());

        match self.engine.change_capture_mode(&self.capture_mode) {
            Ok(_) => {
                let _ = gui_config().get().save_config(false);
            },
            Err(e) => log::warn!("{}", e),
        }
    }

    // 言語の更新
    fn update_language(&mut self, is_initialize: bool) {
        use i18n_embed::LanguageLoader;

        let now_lang = lang_loader().get().current_language();
        if let Some(lang) = gui_config().get().lang.as_ref() {
            if !is_initialize && now_lang.language == lang.language {
                return;
            }
        }

        gui_config().get().lang = Some(now_lang.clone());
        smashbros_resource().get().change_language();
        self.engine.change_language();
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

    fn setup(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame, _storage: Option<&dyn epi::Storage>) {
        smashbros_resource().init(Some(frame));
        gui_config().get().load_config(true).expect("Failed to load config");
        if let Some(lang) = gui_config().get().lang.as_ref() {
            lang_loader().change(lang.clone());
        }
        self.update_language(true);
        if let Some(visuals) = gui_config().get().visuals.as_ref() {
            ctx.set_visuals(visuals.clone());
        }
        self.set_fonts(ctx);

        self.window_battle_information.setup(ctx);
        self.window_battle_history.setup(ctx);
        self.window_configuration.setup(ctx);

        self.window_battle_information.battle_information = WindowBattleInformationGroup::default();
        self.update_battle_informations();
    }

    fn on_exit(&mut self) {
        let _ = gui_config().get().save_config(true);
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame) {
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
        self.update_capture_mode();
        self.update_language(false);

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
    fn name(&self) -> String { fl!(lang_loader().get(), "battle_information") }
    fn show(&mut self, ctx: &egui::CtxRef) {
        egui::Window::new(self.name())
            .default_rect(Self::get_initial_window_rect())
            .show(ctx, |ui| self.ui(ui));
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

// 戦歴タブ
#[derive(PartialEq)]
enum WindowBattleHistoryTab {
    BattleHistory,
    CharacterTable,
}
impl Default for WindowBattleHistoryTab {
    fn default() -> Self { WindowBattleHistoryTab::BattleHistory }
}

// 戦歴
#[derive(Default)]
struct WindowBattleHistory {
    pub battle_information_list: Vec<WindowBattleInformationGroup>,
    pub all_battle_rate_list: LinkedHashMap<String, (f32, i32)>,  // キャラ別, (勝率と試合数)
    window_battle_history_tab: WindowBattleHistoryTab,
    chara_plot_list: HashMap<String, plot::Value>,
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

    const CHARA_IMAGE_ZOOM: f32 = 5.0;
    const CHARA_Y_GROUP_COUNT: i32 = 10;
    pub fn set_data(&mut self, all_battle_rate_list: LinkedHashMap<String, (f32, i32)>) {
        self.all_battle_rate_list = all_battle_rate_list;

        let mut group_count = HashMap::new();
        for (chara_name, (wins_rate, battle_count)) in &self.all_battle_rate_list {
            let y = if *battle_count == 0 {
                // 試合数がないものは表示しない
                continue;
            } else {
                *wins_rate as f64 * 100.0 as f64
            };

            let y_group = y as i32 / Self::CHARA_Y_GROUP_COUNT;
            *group_count.entry(y_group).or_insert(-1) += 1;

            self.chara_plot_list.entry(chara_name.clone()).or_insert(plot::Value::new(
                ((group_count[&y_group] % Self::CHARA_Y_GROUP_COUNT) as f32 * Self::CHARA_IMAGE_ZOOM) as f64,
                y as f64 - (group_count[&y_group] / Self::CHARA_Y_GROUP_COUNT) as f64 * Self::CHARA_IMAGE_ZOOM as f64,
            ));
        }
    }

    // 10戦の履歴表示
    fn battle_history_view(&mut self, ui: &mut egui::Ui) {
        for group in &mut self.battle_information_list {
            group.show_ui(ui);
            ui.separator();
        }
    }

    // キャラ別のグラフ表示
    fn character_table_view(&mut self, ui: &mut egui::Ui) {
        let available_size = ui.available_size();
        GUI::new_grid(GUIIdList::AppearanceTab, 2, egui::Vec2::new(30.0, 5.0))
            .striped(true)
            .show(ui, |ui| {
                plot::Plot::new(GUIIdList::CharacterPlot)
                    .width(available_size.x - 5.0)
                    .height(available_size.y - 5.0)
                    .legend(plot::Legend::default().text_style(egui::TextStyle::Small))
                    .show_axes([false, true])
                    .show(ui, |ui| {
                        ui.line(
                            plot::Line::new(
                                plot::Values::from_values(vec![plot::Value::new(-2.5, 0.0), plot::Value::new(25.5, 0.0), plot::Value::new(47.5, 0.0)]),
                            ).color(egui::Color32::RED)
                            .fill(10.0)
                            .name("負け")
                        );
                        ui.line(
                            plot::Line::new(
                                plot::Values::from_values(vec![plot::Value::new(-2.5, 10.0), plot::Value::new(25.5, 10.0), plot::Value::new(47.5, 10.0)]),
                            ).color(egui::Color32::LIGHT_RED)
                            .fill(40.0)
                            .name("不得手")
                        );
                        ui.line(
                            plot::Line::new(
                                plot::Values::from_values(vec![plot::Value::new(-2.5, 40.0), plot::Value::new(25.5, 40.0), plot::Value::new(47.5, 40.0)]),
                            ).color(egui::Color32::YELLOW)
                            .fill(60.0)
                            .name("丁度")
                        );
                        ui.line(
                            plot::Line::new(
                                plot::Values::from_values(vec![plot::Value::new(-2.5, 60.0), plot::Value::new(25.5, 60.0), plot::Value::new(47.5, 60.0)]),
                            ).color(egui::Color32::LIGHT_GREEN)
                            .fill(90.0)
                            .name("得意")
                        );
                        ui.line(
                            plot::Line::new(
                                plot::Values::from_values(vec![plot::Value::new(-2.5, 90.0), plot::Value::new(25.5, 90.0), plot::Value::new(47.5, 90.0)]),
                            ).color(egui::Color32::LIGHT_BLUE)
                            .fill(100.0)
                            .name("勝ち")
                        );
        
                        for (chara_name, (wins_rate, _battle_count)) in &self.all_battle_rate_list {
                            if !self.chara_plot_list.contains_key(chara_name) {
                                continue;
                            }
                            let chara_texture = match smashbros_resource().get().get_image_handle(chara_name.clone()) {
                                Some(chara_texture) => chara_texture,
                                None => return,
                            };
        
                            ui.image(
                                plot::PlotImage::new(
                                    chara_texture,
                                    self.chara_plot_list[chara_name].clone(),
                                    egui::Vec2::new(Self::CHARA_IMAGE_ZOOM, Self::CHARA_IMAGE_ZOOM),
                                ),
                            );
                            ui.text(plot::Text::new(
                                    plot::Value::new(self.chara_plot_list[chara_name].x, self.chara_plot_list[chara_name].y - Self::CHARA_IMAGE_ZOOM as f64 * 0.6),
                                    &format!("{:3.1}", wins_rate * 100.0)
                                ).color(egui::Color32::WHITE)
                            );
                        };
                    });
            });
    }
}
impl GUIModelTrait for WindowBattleHistory {
    fn name(&self) -> String { fl!(lang_loader().get(), "battle_history") }
    fn show(&mut self, ctx: &egui::CtxRef) {
        egui::Window::new(self.name())
            .default_rect(Self::get_initial_window_rect())
            .vscroll(true)
            .show(ctx, |ui| self.ui(ui));
    }
}
impl GUIViewTrait for WindowBattleHistory {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.window_battle_history_tab, WindowBattleHistoryTab::BattleHistory, fl!(lang_loader().get(), "tab_battle_history"));
            ui.selectable_value(&mut self.window_battle_history_tab, WindowBattleHistoryTab::CharacterTable, fl!(lang_loader().get(), "tab_character_table"));
        });
        ui.separator();

        match self.window_battle_history_tab {
            WindowBattleHistoryTab::BattleHistory => self.battle_history_view(ui),
            WindowBattleHistoryTab::CharacterTable => self.character_table_view(ui),
        }

        ui.allocate_space(ui.available_size());
    }
}

// 設定タブ
#[derive(PartialEq)]
enum ConfigTab {
    Source,
    Appearance,
}
impl Default for ConfigTab {
    fn default() -> Self { ConfigTab::Source }
}

// 設定
#[derive(Default)]
struct WindowConfiguration {
    config_tab: ConfigTab,
    capture_mode: CaptureMode,
    window_caption: String,
    video_device_id: i32,
    video_device_list: Vec<String>,
    window_caption_list: Vec<String>,
    pub now_scene: SceneList,
    pub prev_match_ratio: f64,
}
impl WindowConfiguration {
    // 初期のウィンドウサイズを返す
    pub fn get_initial_window_size() -> egui::Vec2 {
        let parent_size = GUI::get_initial_window_size();

        egui::Vec2::new(parent_size.x, parent_size.y / 10.0 * 2.0 - GUI::get_title_bar_height())
    }

    // 初期のウィンドウサイズ(Rect)を返す
    pub fn get_initial_window_rect() -> egui::Rect {
        egui::Rect::from_min_size(
            egui::Pos2::new(0.0, GUI::get_initial_window_size().y - Self::get_initial_window_size().y),
            Self::get_initial_window_size(),
        )
    }

    // キャプチャモードを設定する
    pub fn set_capture_mode(&mut self, mode: CaptureMode) {
        self.capture_mode = mode;
    }

    // キャプチャモードを取得する
    pub fn get_captured_mode(&self) -> &CaptureMode {
        &self.capture_mode
    }

    // デバイス名からデバイスIDを取得する
    pub fn get_device_id(&self, device_name: String) -> Option<i32> {
        if let Some(id) = self.video_device_list.iter().position(|name| name == &device_name) {
            Some(id as i32)
        } else {
            None
        }
    }

    // キャプチャモードの設定の view を返す
    fn source_settings_view(&mut self, ui: &mut egui::Ui) {
        use crate::capture::{
            CaptureFromWindow,
            CaptureFromVideoDevice,
        };

        egui::ComboBox::from_id_source(GUIIdList::SourceKind)
            .width(ui.available_size().x - 10.0)
            .selected_text(format!("{}", self.capture_mode))
            .show_ui(ui, |ui| {
                if ui.add(egui::SelectableLabel::new( self.capture_mode.is_empty(), fl!(lang_loader().get(), "empty") )).clicked() {
                    self.capture_mode = CaptureMode::new_empty();
                }
                if ui.add(egui::SelectableLabel::new( self.capture_mode.is_window(), fl!(lang_loader().get(), "window") )).clicked() {
                    self.capture_mode = CaptureMode::new_window(self.window_caption.clone());
                    self.window_caption_list = CaptureFromWindow::get_window_list();
                }
                if ui.add(egui::SelectableLabel::new( self.capture_mode.is_video_device(), fl!(lang_loader().get(), "video_device") )).clicked() {
                    self.capture_mode = CaptureMode::new_video_device(self.video_device_id);
                    self.video_device_list = CaptureFromVideoDevice::get_device_list();
                }
                if ui.add(egui::SelectableLabel::new( self.capture_mode.is_desktop(), fl!(lang_loader().get(), "desktop") )).clicked() {
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
                egui::ComboBox::from_id_source(GUIIdList::DeviceList)
                    .selected_text(format!( "{}", video_device_list.get(*device_id as usize).unwrap_or(&fl!(lang_loader().get(), "unselected")) ))
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

    // 外観の設定の view を返す
    fn appearance_settings_view(&mut self, ui: &mut egui::Ui) {
        use i18n_embed::LanguageLoader;
        use crate::resource::Localizations;

        GUI::new_grid(GUIIdList::AppearanceTab, 2, egui::Vec2::new(30.0, 5.0))
            .striped(true)
            .show(ui, |ui| {
                // テーマ
                let style = (*ui.ctx().style()).clone();
                ui.label(fl!(lang_loader().get(), "theme"));
                ui.horizontal(|ui| {
                    if ui.add(egui::SelectableLabel::new(style.visuals == Visuals::dark(), "🌙 Dark")).clicked() {
                        ui.ctx().set_visuals(Visuals::dark());
                        gui_config().get().visuals = Some(Visuals::dark());
                    }
                    if ui.add(egui::SelectableLabel::new(style.visuals == Visuals::light(), "☀ Light")).clicked() {
                        ui.ctx().set_visuals(Visuals::light());
                        gui_config().get().visuals = Some(Visuals::light());
                    }
                });
                ui.end_row();

                // 言語
                let now_lang = lang_loader().get().current_language();
                let lang_list = lang_loader().get().available_languages(&Localizations).unwrap();
                ui.label(fl!(lang_loader().get(), "language"));
                egui::ComboBox::from_id_source(GUIIdList::LanguageKind)
                    .selected_text(format!("{}-{}", now_lang.language, now_lang.region.unwrap().as_str()))
                    .show_ui(ui, |ui| {
                        for lang in &lang_list {
                            if ui.add(egui::SelectableLabel::new(&now_lang == lang, format!("{}-{}", lang.language, lang.region.unwrap().as_str()))).clicked() {
                                lang_loader().change(lang.clone());
                            }
                        }
                    });
                ui.end_row();
            });
    }
}
impl GUIModelTrait for WindowConfiguration {
    fn name(&self) -> String { fl!(lang_loader().get(), "status") }
    fn show(&mut self, ctx: &egui::CtxRef) {
        egui::Window::new( format!("{}:{:?} {}:{:.0}%", self.name(), self.now_scene, fl!(lang_loader().get(), "next"), self.prev_match_ratio * 100.0) )
            .default_rect(Self::get_initial_window_rect())
            .show(ctx, |ui| self.ui(ui));
    }
    fn setup(&mut self, _ctx: &egui::CtxRef) {
        self.video_device_id = -1;
    }
}
impl GUIViewTrait for WindowConfiguration {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.config_tab, ConfigTab::Source, fl!(lang_loader().get(), "tab_source"));
            ui.selectable_value(&mut self.config_tab, ConfigTab::Appearance, fl!(lang_loader().get(), "tab_appearance"));
        });
        ui.separator();

        match self.config_tab {
            ConfigTab::Source => self.source_settings_view(ui),
            ConfigTab::Appearance => self.appearance_settings_view(ui),
        }

        ui.allocate_space(ui.available_size());
    }
}

// 対戦情報グループ
#[derive(Default)]
struct WindowBattleInformationGroup {
    data: Option<SmashbrosData>,
}
impl WindowBattleInformationGroup {
    // BattleInformationGroup を表示するのに必要なデータを設定する
    fn set_data(&mut self, data: SmashbrosData) {
        self.data = Some(data);
    }

    // キャラと順位の表示
    fn show_player_chara(ui: &mut egui::Ui, data: &mut SmashbrosData, player_id: i32) {
        let button = if let Some(order_texture) = smashbros_resource().get().get_order_handle(data.get_order(player_id)) {
            let size = smashbros_resource().get().get_image_size(order_texture).unwrap();
            egui::Button::image_and_text(order_texture, size * egui::Vec2::new(0.25, 0.25), "")
        } else {
            egui::Button::new("?")
        };

        if let Some(chara_image) = GUI::get_chara_image(data.as_ref().get_character(player_id), [32.0, 32.0]) {
            ui.add_sized( [32.0, 32.0], chara_image);
        } else {
            ui.add_sized( [32.0, 32.0], egui::Label::new(format!("{}p", player_id + 1)) );
        }
        egui::Grid::new(GUIIdList::BattleInformationGrid)
            .num_columns(2)
            .spacing(egui::Vec2::new(0.0, 0.0))
            .min_col_width(16.0)
            .min_row_height(8.0)
            .show(ui, |ui| {
                ui.end_row();

                if ui.add_sized( [20.0, 20.0], button ).clicked() {
                    if !data.is_finished_battle() {
                        return;
                    }

                    // 順位の変更
                    if data.all_decided_order() {
                        // どちらの順位も確定している場合は交換
                        if data.get_order(0) == 1 {
                            data.set_order(0, 2);
                            data.set_order(1, 1);
                        } else {
                            data.set_order(0, 1);
                            data.set_order(1, 2);
                        }
                    } else {
                        // どちらかの順位がわからない場合は固定 [1p -> 1, 2p -> 2]
                        data.set_order(0, 1);
                        data.set_order(1, 2);
                    }

                    data.update_battle();
                }
            });
    }

    // ストックの表示 (3 ストック以下ならアイコン表示、それ以上ならアイコンと数値を表示)
    fn show_player_stock(ui: &mut egui::Ui, data: &mut SmashbrosData, player_id: i32) {
        let stock = data.get_stock(player_id);
        for i in 0..3 {
            if (0 != stock) && (i < stock || 0 == i) {
                if let Some(chara_image) = GUI::get_chara_image(data.get_character(player_id), [16.0, 16.0]) {
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

    fn show_ui(&mut self, ui: &mut egui::Ui) {
        /*
         * [対戦情報グループ]
         * .1pキャラアイコン vs 2pキャラアイコン
         * .ルール(アイコンにしたい), 時間
         * .ストック(アイコンにしたい)
         */
        let data = match self.data.as_mut() {
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
                        Self::show_player_chara(ui, data, 0);
                        ui.add_sized( [16.0, 16.0], egui::Label::new("vs") );
                        Self::show_player_chara(ui, data, 1);
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
                        Self::show_player_stock(ui, data, 0);
                        ui.end_row();
                        Self::show_player_stock(ui, data, 1);
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
    win_rate: (f32, i32),
}
impl WindowWinsGraph {
    fn set_data(&mut self, data: SmashbrosData, data_list: Vec<SmashbrosData>, wins_lose: (i32, i32), win_rate: (f32, i32)) {
        let mut data_list = data_list;
        data_list.reverse();
        self.point_list = data_list.iter().enumerate().filter_map(|(x, data)| {
            if data.get_power(0) < 0 {
                return None;
            }

            Some(plot::Value::new(x as f64, data.get_power(0) as f64))
        }).collect::<Vec<plot::Value>>();

        self.now_data = Some(data);
        let last = match data_list.last() {
            Some(last) => last,
            None => return,
        };

        self.last_power = last.get_power(0);
        self.wins_lose = wins_lose;
        self.win_rate = win_rate;
    }

    fn show_ui(&self, ui: &mut egui::Ui) {
        let points_values = plot::Values::from_values(self.point_list.clone());
        let line_values = plot::Values::from_values(self.point_list.clone());

        GUI::new_grid("wins_graph_group", 2, egui::Vec2::new(0.0, 0.0))
            .min_col_width(120.0)
            .show(ui, |ui| {
                GUI::new_grid("wins_group", 6, egui::Vec2::new(0.0, 0.0))
                    .show(ui, |ui| {
                        let now_data = match &self.now_data {
                            Some(data) => data,
                            None => return,
                        };
                        if !now_data.is_decided_character_name(0) || !now_data.is_decided_character_name(1) {
                            return;
                        }

                        // 対キャラクター勝率
                        if let Some(image) = GUI::get_chara_image(now_data.as_ref().get_character(0), [16.0, 16.0]) {
                            ui.add_sized( [16.0, 16.0], image);
                        } else {
                            ui.add_sized( [16.0, 16.0], egui::Label::new("1p"));
                        }
                        ui.add_sized( [16.0, 16.0], egui::Label::new("vs"));
                        if let Some(image) = GUI::get_chara_image(now_data.as_ref().get_character(1), [16.0, 16.0]) {
                            ui.add_sized( [16.0, 16.0], image);
                        } else {
                            ui.add_sized( [16.0, 16.0], egui::Label::new("2p"));
                        }
                        ui.add_sized( [16.0, 16.0], egui::Label::new(format!("{:3.1}% / {}", 100.0 * self.win_rate.0, self.win_rate.1)));
                        ui.end_row();
                    });
                
                // 世界戦闘力グラフの表示
                plot::Plot::new(GUIIdList::PowerPlot)
                    .width(GUI::get_initial_window_size().x / 2.0)
                    .height(40.0)
                    .legend(plot::Legend::default())
                    .view_aspect(1.0)
                    .show_axes([false, false])
                    .show(ui, |ui| {
                        ui.line(plot::Line::new(line_values).color(egui::Color32::WHITE));
                        ui.points(
                            plot::Points::new(points_values).radius(2.0)
                                // Light モードのときだけ点を白にすることで、GSP だけをクリッピングして表示しやすいようにする
                                .color(if ui.ctx().style().visuals == Visuals::dark() { egui::Color32::RED } else { egui::Color32::WHITE })
                                .name(format!("{}\n{}", fl!(lang_loader().get(), "GSP"), self.last_power))
                        );
                    });
            });
    }
}
 
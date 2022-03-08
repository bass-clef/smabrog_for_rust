
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
use crate::engine::{
    SmashBrogEngine,
    SMASHBROS_ENGINE,
};
use crate::resource::{
    SoundType,
    BATTLE_HISTORY,
    GUI_CONFIG,
    LANG_LOADER,
    SMASHBROS_RESOURCE,
    SOUND_MANAGER,
};
use crate::scene::SceneList;


pub async fn run_gui() -> anyhow::Result<()> {
    let mut native_options = eframe::NativeOptions::default();
    native_options.icon_data = Some(GUI::get_icon_data());
    native_options.initial_window_size = Some(GUI::get_initial_window_size());
    native_options.resizable = false;
    native_options.drag_and_drop_support = true;

    let app = GUI::new();

    eframe::run_native(Box::new(app), native_options)
}


// GUIの種類, is_source に指定するのに必要
#[derive(std::hash::Hash)]
enum GUIIdList {
    SourceTab,
    AppearanceTab,
    DetailTab,
    CustomizeTab,
    SourceKind,
    LanguageComboBox,
    FontComboBox,
    BgmDeviceComboBox,
    BgmSessionComboBox,

    WindowList,
    DeviceList,

    BattleInformationGrid,
    BattleInformationChildGrid,
    CharacterHistoryGrid,
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
    capture_mode: CaptureMode,
    window_battle_information: WindowBattleInformation,
    window_battle_history: WindowBattleHistory,
    window_configuration: WindowConfiguration,
}
impl GUI {
    fn new() -> Self {
        Self {
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
        if let Some(chara_texture) = SMASHBROS_RESOURCE().get_mut().get_image_handle(chara_name) {
            return Some(egui::Image::new(chara_texture, egui::Vec2::new(size[0], size[1])));
        }

        None
    }

    // 初期化ウィンドウサイズを返す
    pub fn get_initial_window_size() -> egui::Vec2 { egui::Vec2::new(256f32, 720f32) }

    // タイトルバーの高さを返す
    pub fn get_title_bar_height() -> f32 { 32.0 }

    // フォントの設定
    pub fn set_font(ctx: &egui::CtxRef, font_family: Option<String>, font_size: i32) {
        let default_fonts = (
            "Mamelon".to_string(),
            egui::FontData::from_static(include_bytes!("../fonts/Mamelon-5-Hi-Regular.otf"))
        );

        let font_datas = match font_family {
            Some(font_family) => {
                if font_family == "Mamelon".to_string() {
                    default_fonts
                } else {
                    let family_handle = font_kit::source::SystemSource::new().select_family_by_name(&font_family).expect("Font not found");
                    let font = family_handle.fonts()[0].load().expect("Failed load font");
    
                    (font_family, egui::FontData::from_owned(font.copy_font_data().unwrap().to_vec()) )
                }
            },
            None => default_fonts,
        };

        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(font_datas.0.clone(), font_datas.1);
        fonts.fonts_for_family
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, font_datas.0.clone());

        fonts.family_and_size.insert(egui::TextStyle::Heading, (egui::FontFamily::Proportional, 16.0));
        fonts.family_and_size.insert(egui::TextStyle::Button, (egui::FontFamily::Proportional, 12.0));
        fonts.family_and_size.insert(egui::TextStyle::Body, (egui::FontFamily::Proportional, 12.0));

        let font_size_base = font_size as f32;
        fonts.family_and_size.insert(egui::TextStyle::Small, (egui::FontFamily::Proportional, font_size_base));

        ctx.set_fonts(fonts);

        GUI_CONFIG().get_mut().font_size = Some(font_size);
        GUI_CONFIG().get_mut().font_family = Some(font_datas.0);
    }

    // 幅が 0 の egui::Grid を返す
    pub fn new_grid<T>(id_source: T, columns: usize, spacing: egui::Vec2) -> egui::Grid where T: std::hash::Hash {
        egui::Grid::new(id_source)
            .num_columns(columns)
            .spacing(spacing)
            .min_col_width(0.0)
            .min_row_height(0.0)
    }

    // デフォルトフォントの設定
    fn set_default_font(&mut self, ctx: &egui::CtxRef) {
        Self::set_font(ctx, GUI_CONFIG().get_mut().font_family.clone(), GUI_CONFIG().get_mut().font_size.unwrap_or(12));

        self.window_configuration.font_size = GUI_CONFIG().get_mut().font_size.clone().unwrap();
        self.window_configuration.font_family = GUI_CONFIG().get_mut().font_family.clone().unwrap();
    }

    // 対戦情報の更新
    fn update_battle_informations(&mut self) {
        // 検出状態
        self.window_configuration.now_scene = SMASHBROS_ENGINE().get_mut().get_captured_scene();
        self.window_configuration.prev_match_ratio = SMASHBROS_ENGINE().get_mut().get_prev_match_ratio();

        if GUI_CONFIG().get_mut().gui_state_config.show_captured {
            // 検出しているフレームを表示
            let _ = opencv::highgui::imshow("smabrog - captured", SMASHBROS_ENGINE().get_mut().get_now_image());
        }

        // 対戦中情報
        self.window_battle_information.battle_information.set_data( SMASHBROS_ENGINE().get_mut().ref_now_data().clone() );

        // 下記から、戦歴情報の変動があったときだけにしたい処理
        if !SMASHBROS_ENGINE().get_mut().is_update_now_data() {
            return;
        }

        // 戦歴
        self.window_battle_history.battle_information_list.clear();
        let data_latest = SMASHBROS_ENGINE().get_mut().get_data_latest();
        for data in data_latest.clone() {
            let mut battle_information = WindowBattleInformationGroup::default();
            battle_information.set_data(data);

            self.window_battle_history.battle_information_list.push(battle_information);
        }
        let all_data_list = SMASHBROS_ENGINE().get_mut().get_data_all_by_now_chara();
        self.window_battle_history.set_data(
            SmashBrogEngine::get_wins_by_data_list_groupby_character(&all_data_list));

        let chara_data_list = SMASHBROS_ENGINE().get_mut().get_data_latest_by_now_chara();
        self.window_battle_information.wins_graph.set_data(
            SMASHBROS_ENGINE().get_mut().get_now_data(),
            data_latest.clone(),
            SmashBrogEngine::get_win_lose_by_data_list(&data_latest),
            SmashBrogEngine::get_wins_by_data_list(&chara_data_list),
            WinsGraphKind::Gsp
        );
    }

    // BGM の更新
    fn update_bgm(&mut self) {
        self.window_configuration.update_bgm();
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
                    *caption_name = GUI_CONFIG().get_mut().capture_win_caption.clone();
                },
                CaptureMode::VideoDevice(_, device_id, _) => {
                    *device_id = self.window_configuration.get_device_id(
                        GUI_CONFIG().get_mut().capture_device_name.clone()
                    ).unwrap_or(-1);
                },
                _ => (),
            }
        }

        self.window_configuration.set_capture_mode(self.capture_mode.clone());

        match SMASHBROS_ENGINE().get_mut().change_capture_mode(&self.capture_mode) {
            Ok(_) => {
                let _ = GUI_CONFIG().get_mut().save_config(false);
            },
            Err(e) => log::warn!("{}", e),
        }
    }

    // 言語の更新
    fn update_language(&mut self, is_initialize: bool) {
        use i18n_embed::LanguageLoader;

        let now_lang = LANG_LOADER().get().current_language();
        if let Some(lang) = GUI_CONFIG().get_mut().lang.as_ref() {
            if !is_initialize && now_lang.language == lang.language {
                return;
            }
        }

        GUI_CONFIG().get_mut().lang = Some(now_lang.clone());
        SMASHBROS_RESOURCE().get_mut().change_language();
        SMASHBROS_ENGINE().get_mut().change_language();
    }
}
impl epi::App for GUI {
    fn name(&self) -> &str { "smabrog" }

    fn setup(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame, _storage: Option<&dyn epi::Storage>) {
        SMASHBROS_RESOURCE().init(Some(frame));
        GUI_CONFIG().get_mut().load_config(true).expect("Failed to load config");
        if let Some(lang) = GUI_CONFIG().get_mut().lang.as_ref() {
            LANG_LOADER().change(lang.clone());
        }
        self.update_language(true);
        self.set_default_font(ctx);
        self.window_battle_information.setup(ctx);
        self.window_battle_history.setup(ctx);
        self.window_configuration.setup(ctx);

        self.window_battle_information.battle_information = WindowBattleInformationGroup::default();
        self.update_battle_informations();

        SMASHBROS_ENGINE().get_mut().registory_scene_event(
            SceneList::Unknown,
            SceneList::DecidedRules,
            Box::new(|smashbros_data| {
                // 試合中、最大ストックが警告未満になったら警告音を再生
                if smashbros_data.is_decided_max_stock(0) {
                    if smashbros_data.get_max_stock(0) < GUI_CONFIG().get_mut().gui_state_config.stock_warning_under {
                        if !GUI_CONFIG().get_mut().stock_alert_command.is_empty() {
                            let command_result = std::process::Command::new("cmd")
                                .args(["/K", "start", &GUI_CONFIG().get_mut().stock_alert_command])
                                .output();
                            
                            match command_result {
                                Ok(result) => {
                                    if result.status.success() {
                                        log::info!("stock alert command: {}", String::from_utf8_lossy(&result.stdout));
                                    } else {
                                        log::warn!("Failed to execute command: {}", String::from_utf8_lossy(&result.stderr));
                                    }
                                },
                                Err(e) => log::error!("{}", e),
                            }
                        }
                    }
                }
            })
        );

        SMASHBROS_ENGINE().get_mut().registory_scene_event(
            SceneList::Unknown,
            SceneList::DecidedBgm,
            Box::new(|smashbros_data| {
                // BGM が確定していて, BGM が許可されていないなら、変わりの BGM を再生する
                if !SMASHBROS_RESOURCE().get_mut().bgm_list.is_empty() {
                    if smashbros_data.is_decided_bgm_name() {
                        if let Some(is_playble) = SMASHBROS_RESOURCE().get_mut().bgm_list.get(&smashbros_data.get_bgm_name()) {
                            if !*is_playble {
                                SOUND_MANAGER().get_mut().play_bgm_random();
                            }
                        }
                    }
                }
            })
        );


        let bgm_callback = Box::new(|_smashbros_data: &mut SmashbrosData| {
            // もし BGM が再生中なら止める
            if SOUND_MANAGER().get_mut().is_playing(Some(SoundType::Bgm)) {
                SOUND_MANAGER().get_mut().stop(Some(SoundType::Bgm));
            }
        });
        SMASHBROS_ENGINE().get_mut().registory_scene_event(SceneList::GamePlaying, SceneList::GameEnd, bgm_callback.clone());
        SMASHBROS_ENGINE().get_mut().registory_scene_event(SceneList::GamePlaying, SceneList::ReadyToFight, bgm_callback.clone());
    }

    fn on_exit(&mut self) {
        let _ = GUI_CONFIG().get_mut().save_config(true);
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame) {
        self.update_battle_informations();
        self.update_capture_mode();
        self.update_language(false);
        self.update_bgm();
        
        // 動作
        if let Err(e) = SMASHBROS_ENGINE().get_mut().update() {
            // quit
            // TODO:ゆくゆくはエラー回復とかもできるようにしたい
            log::error!("quit. [{}]", e);
            frame.quit();
            return;
        }

        // 表示 (戦歴が最前面になるように下から描画)
        self.window_configuration.show(ctx);
        self.window_battle_history.show(ctx);
        self.window_battle_information.show(ctx);

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
    fn name(&self) -> String { fl!(LANG_LOADER().get(), "battle_information") }
    fn show(&mut self, ctx: &egui::CtxRef) {
        egui::Window::new(self.name())
            .default_rect(Self::get_initial_window_rect())
            .show(ctx, |ui| self.ui(ui));
    }
}
impl GUIViewTrait for WindowBattleInformation {
    fn ui(&mut self, ui: &mut egui::Ui) {
        if GUI_CONFIG().get_mut().gui_state_config.battling {
            self.battle_information.show_ui(ui, |_ui| {});
            ui.separator();
        }
        self.wins_graph.show_ui( ui, fl!(LANG_LOADER().get(), "gsp") );

        ui.allocate_space(ui.available_size());
    }
}

// 戦歴タブ
#[derive(PartialEq)]
enum WindowBattleHistoryTab {
    BattleHistory,
    CharacterTable,
    CharacterHistory,
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
    find_character_list: Vec<String>,
    character_history_list: Vec<WindowBattleInformationGroup>,
    character_history_graph: WindowWinsGraph,
    is_exact_match: bool,
    max_battle_count: f32,
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
    const CHARA_TABLE_WIDTH: f64 = 50.0 - 2.5;
    pub fn set_data(&mut self, all_battle_rate_list: LinkedHashMap<String, (f32, i32)>) {
        self.all_battle_rate_list = all_battle_rate_list;

        self.max_battle_count = self.all_battle_rate_list.iter().fold(0, |max, (_, (_, battle_count))| {
            if &max < battle_count { *battle_count } else { max }
        }) as f32;

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

    // N 戦の履歴表示
    fn battle_history_view(&mut self, ui: &mut egui::Ui) {
        if WindowBattleInformationGroup::show_group_list_with_delete(ui, &mut self.battle_information_list) {
            SMASHBROS_ENGINE().get_mut().update_latest_n_data();
        }
    }

    // キャラ表のラベルの表示
    fn show_table_label(ui: &mut plot::PlotUi) {
        ui.line(
            plot::Line::new(
                plot::Values::from_values(vec![plot::Value::new(-2.5, 0.0), plot::Value::new(25.5, 0.0), plot::Value::new(Self::CHARA_TABLE_WIDTH, 0.0)]),
            ).color(egui::Color32::RED)
            .fill(10.0)
            .name(fl!(LANG_LOADER().get(), "losing")),
        );
        ui.line(
            plot::Line::new(
                plot::Values::from_values(vec![plot::Value::new(-2.5, 10.0), plot::Value::new(25.5, 10.0), plot::Value::new(Self::CHARA_TABLE_WIDTH, 10.0)]),
            ).color(egui::Color32::LIGHT_RED)
            .fill(40.0)
            .name(fl!(LANG_LOADER().get(), "not_good"))
        );
        ui.line(
            plot::Line::new(
                plot::Values::from_values(vec![plot::Value::new(-2.5, 40.0), plot::Value::new(25.5, 40.0), plot::Value::new(Self::CHARA_TABLE_WIDTH, 40.0)]),
            ).color(egui::Color32::YELLOW)
            .fill(60.0)
            .name(fl!(LANG_LOADER().get(), "just"))
        );
        ui.line(
            plot::Line::new(
                plot::Values::from_values(vec![plot::Value::new(-2.5, 60.0), plot::Value::new(25.5, 60.0), plot::Value::new(Self::CHARA_TABLE_WIDTH, 60.0)]),
            ).color(egui::Color32::LIGHT_GREEN)
            .fill(90.0)
            .name(fl!(LANG_LOADER().get(), "good"))
        );
        ui.line(
            plot::Line::new(
                plot::Values::from_values(vec![plot::Value::new(-2.5, 90.0), plot::Value::new(25.5, 90.0), plot::Value::new(Self::CHARA_TABLE_WIDTH, 90.0)]),
            ).color(egui::Color32::LIGHT_BLUE)
            .fill(100.0)
            .name(fl!(LANG_LOADER().get(), "winning"))
        );
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
                    .legend(plot::Legend::default().text_style(egui::TextStyle::Body))
                    .show_axes([false, true])
                    .show(ui, |ui| {
                        Self::show_table_label(ui);
                        for (chara_name, (wins_rate, _battle_count)) in &self.all_battle_rate_list {
                            if !self.chara_plot_list.contains_key(chara_name) {
                                continue;
                            }
                            let chara_texture = match SMASHBROS_RESOURCE().get_mut().get_image_handle(chara_name.clone()) {
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
                                .style(egui::TextStyle::Body)
                            );
                        };
                    });
            });
    }

    // 対キャラの戦歴表示
    fn character_history_view(&mut self, ui: &mut egui::Ui) {
        use crate::resource::SmashbrosResource;

        let one_width = ui.available_size().x / 4.0;
        GUI::new_grid(GUIIdList::CharacterHistoryGrid, 4, egui::Vec2::new(5.0, 0.0))
            .show(ui, |ui| {
                ui.checkbox( &mut self.is_exact_match, fl!(LANG_LOADER().get(), "exact_match") );
                ui.add_sized([one_width, 18.0],
                    egui::TextEdit::singleline(&mut self.find_character_list[0])
                        .hint_text("1p")
                );
                ui.add_sized([one_width, 18.0],
                    egui::TextEdit::singleline(&mut self.find_character_list[1])
                        .hint_text("2p")
                );
                if ui.button(fl!( LANG_LOADER().get(), "search" )).clicked() {
                    // キャラ名推測をする
                    self.find_character_list = self.find_character_list.iter_mut().map(|chara_name| {
                        if let Some((new_chara_name, _)) = SmashbrosResource::convert_character_name(chara_name.to_uppercase()) {
                            return new_chara_name;
                        }

                        chara_name.clone()
                    }).collect();
                    log::info!("search character history: {:?}", self.find_character_list);

                    if let Some(data_list) = BATTLE_HISTORY().get_mut().find_data_by_chara_list(self.find_character_list.clone(), 100, !self.is_exact_match) {
                        self.character_history_list.clear();
                        for data in data_list.clone() {
                            let mut battle_information = WindowBattleInformationGroup::default();
                            battle_information.set_data(data);
                            self.character_history_list.push(battle_information);
                        }

                        if !data_list.is_empty() {
                            self.character_history_graph.set_data(
                                data_list[0].clone(),
                                data_list.clone(),
                                SmashBrogEngine::get_win_lose_by_data_list(&data_list),
                                SmashBrogEngine::get_wins_by_data_list(&data_list),
                                WinsGraphKind::Rate
                            );
                        }
                    }
                    if self.character_history_list.is_empty() {
                        // 検索結果がなにもない場合は default の SmashbrosData を突っ込む
                        let mut battle_information = WindowBattleInformationGroup::default();
                        battle_information.set_data( SmashbrosData::default() );
                        self.character_history_list.push(battle_information);
                    }
                }
            });

        ui.separator();
        self.character_history_graph.show_ui( ui, fl!(LANG_LOADER().get(), "passage") );
        
        ui.separator();
        if WindowBattleInformationGroup::show_group_list_with_delete(ui, &mut self.character_history_list) {
            SMASHBROS_ENGINE().get_mut().update_latest_n_data();
            SMASHBROS_ENGINE().get_mut().update_chara_find_data();
        }
    }
}
impl GUIModelTrait for WindowBattleHistory {
    fn setup(&mut self, _ctx: &egui::CtxRef) {
        self.find_character_list = vec![String::new(); 2];
        self.is_exact_match = true;
    }
    fn name(&self) -> String { fl!(LANG_LOADER().get(), "battle_history") }
    fn show(&mut self, ctx: &egui::CtxRef) {
        egui::Window::new(self.name())
            .default_rect(Self::get_initial_window_rect())
            .vscroll(true)
            .hscroll(true)
            .show(ctx, |ui| self.ui(ui));
    }
}
impl GUIViewTrait for WindowBattleHistory {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.window_battle_history_tab, WindowBattleHistoryTab::BattleHistory, format!("{} {}", self.battle_information_list.len(), fl!(LANG_LOADER().get(), "tab_battle_history")));
            ui.selectable_value(&mut self.window_battle_history_tab, WindowBattleHistoryTab::CharacterTable, fl!(LANG_LOADER().get(), "tab_character_table"));
            ui.selectable_value(&mut self.window_battle_history_tab, WindowBattleHistoryTab::CharacterHistory, fl!(LANG_LOADER().get(), "tab_character_history"));
        });
        ui.separator();

        match self.window_battle_history_tab {
            WindowBattleHistoryTab::BattleHistory => self.battle_history_view(ui),
            WindowBattleHistoryTab::CharacterTable => self.character_table_view(ui),
            WindowBattleHistoryTab::CharacterHistory => self.character_history_view(ui),
        }

        ui.allocate_space(ui.available_size());
    }
}

// 設定タブ
#[derive(PartialEq)]
enum ConfigTab {
    Source,
    Appearance,
    Detail,
    Customize,
}
impl Default for ConfigTab {
    fn default() -> Self { ConfigTab::Source }
}
// 設定
struct WindowConfiguration {
    config_tab: ConfigTab,
    capture_mode: CaptureMode,

    window_caption_list: Vec<String>,
    window_caption: String,
    video_device_list: Vec<String>,
    video_device_id: i32,
    font_family_list: Vec<String>,
    bgm_device_list: HashMap<String, HashMap<String, wasapi::SimpleAudioVolume>>,
    before_volume: Option<f32>,

    pub now_scene: SceneList,
    pub prev_match_ratio: f64,
    pub font_family: String,
    pub font_size: i32,
}
impl Default for WindowConfiguration {
    fn default() -> Self { Self::new() }
}
impl WindowConfiguration {
    fn new() -> Self {
        // WASAPI のほうを先に初期化しないと rodio と競合するっぽい
        Self {
            config_tab: ConfigTab::Source,

            capture_mode: CaptureMode::default(),

            window_caption_list: Vec::new(),
            window_caption: String::new(),
            video_device_list: Vec::new(),
            video_device_id: 0,
            font_family_list: Vec::new(),
            bgm_device_list: Self::init_wasapi(),
            before_volume: None,

            now_scene: SceneList::default(),
            prev_match_ratio: 0.0,
            font_family: String::new(),
            font_size: 0,
        }
    }

    /// WASAPI の初期化と BGM デバイスのリストを作成する
    pub fn init_wasapi() -> HashMap<String, HashMap<String, wasapi::SimpleAudioVolume>> {
        wasapi::initialize_sta().expect("Failed to initialize WASAPI.");
        let mut bgm_device_list: HashMap<String, HashMap<String, wasapi::SimpleAudioVolume>> = HashMap::new();
        let device_collection = wasapi::DeviceCollection::new(&wasapi::Direction::Render).expect("Failed get eRender devices.");
        for device_id in 0..device_collection.get_nbr_devices().unwrap_or(0) {
            let device = device_collection.get_device_at_index(device_id).unwrap();
            let device_name = match device.get_friendlyname() {
                Ok(name) => name,
                Err(_) => continue,
            };
            let session_manager = match device.get_sessionmanager() {
                Ok(session_manager) => session_manager,
                Err(_) => continue,
            };

            for i in 0..session_manager.get_session_count().unwrap_or(0) {
                let session = match session_manager.get_audiosessioncontrol(i) {
                    Ok(session) => session,
                    Err(_) => continue,
                };
                let process_name = match session.get_process_name() {
                    Ok(name) => if session.get_process_id().unwrap_or(0) == 0 {
                        device_name.clone()
                    } else {
                        name
                    },
                    Err(_) => continue,
                };
                let simple_audio_volume = match session.get_simpleaudiovolume() {
                    Ok(simple_audio_volume) => simple_audio_volume,
                    Err(_) => continue,
                };

                bgm_device_list.entry(device_name.clone()).or_insert(HashMap::new())
                    .insert(process_name, simple_audio_volume);
            }
        }

        bgm_device_list
    }

    // 文字列を任意の長さに調節して、それ以下は「...」をつけるキャプションを作成する
    fn get_small_caption(caption: String, length: usize) -> String {
        // 長すぎると表示が崩れるので短くする(UTF-8だと境界がおかしいと None になるっぽいので 4byte分見る)
        let l = std::cmp::min(caption.len(), length);
        let mut selected_text = caption.get(0..l).unwrap_or(
            caption.get(0..(l+1)).unwrap_or(
                caption.get(0..(l+2)).unwrap_or(
                    caption.get(0..(l+3)).unwrap_or("")
                )
            )
        ).to_string();
        selected_text += if selected_text.len() == caption.len() { "" } else { "..." };

        selected_text
    }

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

    pub fn update_bgm(&mut self) {
        // 選択されているデバイスを取得
        let bgm_device_name = match GUI_CONFIG().get_mut().bgm_device_name.as_ref() {
            Some(bgm_device_name) => bgm_device_name,
            None => return,
        };
        let bgm_session_name = match GUI_CONFIG().get_mut().bgm_session_name.as_ref() {
            Some(bgm_session_name) => bgm_session_name,
            None => return,
        };
        let simple_audio_volume = match self.bgm_device_list.get(bgm_device_name) {
            Some(device) => match device.get(bgm_session_name) {
                Some(simple_audio_volume) => simple_audio_volume,
                None => return,
            },
            None => return,
        };

        if let Some(before_volume) = self.before_volume {
            // BGM が停止していて、音量を変更した痕跡があるならもとに戻す
            if SOUND_MANAGER().get_mut().is_playing(Some(SoundType::Bgm)) {
                return;
            }
            if let Err(err) = simple_audio_volume.set_master_volume(before_volume) {
                log::error!("{}", err);
            }
            self.before_volume = None;
        } else {
            // 音量を変更する必要があり、BGM が再生中なら、現在の音量を記憶して変更する
            if GUI_CONFIG().get_mut().gui_state_config.disable_volume == 1.0 {
                return;
            }
            if !SOUND_MANAGER().get_mut().is_playing(Some(SoundType::Bgm)) {
                return;
            }
            self.before_volume = Some(simple_audio_volume.get_master_volume().unwrap_or(1.0));
            if let Err(err) = simple_audio_volume.set_master_volume(self.before_volume.unwrap_or(1.0) * GUI_CONFIG().get_mut().gui_state_config.disable_volume) {
                log::error!("{}", err);
            }
        }
    }

    // キャプチャモードの設定の view を返す
    fn source_settings_view(&mut self, ui: &mut egui::Ui) {
        use crate::capture::{
            CaptureFromWindow,
            CaptureFromVideoDevice,
        };

        GUI::new_grid(GUIIdList::SourceTab, 1, egui::Vec2::new(0.0, 5.0))
            .striped(true)
            .max_col_width(ui.available_size().x - 5.0)
            .show(ui, |ui| {
                egui::ComboBox::from_id_source(GUIIdList::SourceKind)
                .selected_text(format!("{}", self.capture_mode))
                .show_ui(ui, |ui| {
                    if ui.add(egui::SelectableLabel::new( self.capture_mode.is_empty(), fl!(LANG_LOADER().get(), "empty") )).clicked() {
                        self.capture_mode = CaptureMode::new_empty();
                    }
                    if ui.add(egui::SelectableLabel::new( self.capture_mode.is_window(), fl!(LANG_LOADER().get(), "window") )).clicked() {
                        self.capture_mode = CaptureMode::new_window(self.window_caption.clone());
                        self.window_caption_list = CaptureFromWindow::get_window_list();
                    }
                    if ui.add(egui::SelectableLabel::new( self.capture_mode.is_video_device(), fl!(LANG_LOADER().get(), "video_device") )).clicked() {
                        self.capture_mode = CaptureMode::new_video_device(self.video_device_id);
                        self.video_device_list = CaptureFromVideoDevice::get_device_list();
                    }
                    if ui.add(egui::SelectableLabel::new( self.capture_mode.is_desktop(), fl!(LANG_LOADER().get(), "desktop") )).clicked() {
                        self.capture_mode = CaptureMode::new_desktop();
                    }
                });
                ui.end_row();

                let Self {
                    video_device_list,
                    window_caption_list,
                    window_caption,
                    video_device_id,
                    ..
                } = self;
                match &mut self.capture_mode {
                    CaptureMode::Window(_, cm_window_caption) => {
                        egui::ComboBox::from_id_source(GUIIdList::WindowList)
                            .selected_text(Self::get_small_caption(cm_window_caption.clone(), 40))
                            .width(ui.available_size().x - 10.0)
                            .show_ui(ui, |ui| {
                                for wc in window_caption_list {
                                    if ui.add(egui::SelectableLabel::new( wc == cm_window_caption, wc.as_str() )).clicked() {
                                        *cm_window_caption = wc.clone();
                                        *window_caption = wc.clone();
                                    }
                                }
                            });
                    },
                    CaptureMode::VideoDevice(_, cm_device_id, _) => {
                        let selected_text = format!( "{}", video_device_list.get(*cm_device_id as usize).unwrap_or(&fl!(LANG_LOADER().get(), "unselected")) );
                        let selected_text = Self::get_small_caption(selected_text.clone(), 40);
                        egui::ComboBox::from_id_source(GUIIdList::DeviceList)
                            .selected_text(selected_text)
                            .width(ui.available_size().x - 10.0)
                            .show_ui(ui, |ui| {
                                for (id, name) in video_device_list.iter().enumerate() {
                                    if ui.add(egui::SelectableLabel::new(*cm_device_id == id as i32, name)).clicked() {
                                        *cm_device_id = id as i32;
                                        *video_device_id = id as i32;
                                    }
                                }
                            });
                    },
                    _ => (),
                }
                ui.end_row();
        
                // 状態の表示
                ui.checkbox(
                    &mut GUI_CONFIG().get_mut().gui_state_config.show_captured,
                    format!(
                        "{}:{:?} {}:{:.0}%", fl!(LANG_LOADER().get(), "status"), self.now_scene,
                        fl!(LANG_LOADER().get(), "next"), self.prev_match_ratio * 100.0
                    )
                );
            });
    }

    // 外観の設定の view を返す
    fn appearance_settings_view(&mut self, ui: &mut egui::Ui) {
        use i18n_embed::LanguageLoader;
        use crate::resource::Localizations;

        GUI::new_grid(GUIIdList::AppearanceTab, 2, egui::Vec2::new(10.0, 5.0))
            .striped(true)
            .max_col_width(ui.available_size().x / 2.0)
            .show(ui, |ui| {
                // テーマ
                let style = (*ui.ctx().style()).clone();
                ui.label(fl!(LANG_LOADER().get(), "theme"));
                ui.horizontal(|ui| {
                    if ui.add(egui::SelectableLabel::new(style.visuals == Visuals::dark(), "🌙 Dark")).clicked() {
                        ui.ctx().set_visuals(Visuals::dark());
                        GUI_CONFIG().get_mut().visuals = Some(Visuals::dark());
                    }
                    if ui.add(egui::SelectableLabel::new(style.visuals == Visuals::light(), "☀ Light")).clicked() {
                        ui.ctx().set_visuals(Visuals::light());
                        GUI_CONFIG().get_mut().visuals = Some(Visuals::light());
                    }
                });
                ui.end_row();

                // 言語
                let now_lang = LANG_LOADER().get().current_language();
                let lang_list = LANG_LOADER().get().available_languages(&Localizations).unwrap();
                ui.label(fl!(LANG_LOADER().get(), "language"));
                egui::ComboBox::from_id_source(GUIIdList::LanguageComboBox)
                    .selected_text(format!("{}-{}", now_lang.language, now_lang.region.unwrap().as_str()))
                    .show_ui(ui, |ui| {
                        for lang in &lang_list {
                            if ui.add(egui::SelectableLabel::new(&now_lang == lang, format!("{}-{}", lang.language, lang.region.unwrap().as_str()))).clicked() {
                                LANG_LOADER().change(lang.clone());
                            }
                        }
                    });
                ui.end_row();

                // フォント
                use eframe::egui::Widget;
                ui.label(fl!(LANG_LOADER().get(), "font"));
                ui.scope(|ui| {
                    // フォントサイズ
                    if egui::DragValue::new(&mut self.font_size)
                        .clamp_range(1..=1000)
                        .ui(ui).changed()
                    {
                        GUI::set_font(ui.ctx(), Some(self.font_family.clone()), self.font_size);
                    }

                    // フォント一覧
                    let selected_font = Self::get_small_caption(self.font_family.clone(), 12);
                    egui::ComboBox::from_id_source(GUIIdList::FontComboBox)
                        .selected_text(selected_font)
                        .show_ui(ui, |ui| {
                            for font_family in &self.font_family_list {
                                if ui.selectable_value(&mut self.font_family, font_family.clone(), font_family.clone()).changed() {
                                    GUI::set_font(ui.ctx(), Some(self.font_family.clone()), self.font_size);
                                }
                            }
                    });
                });
            });
    }

    // 詳細の設定の view を返す
    fn detail_settings_view(&mut self, ui: &mut egui::Ui) {
        use eframe::egui::Widget;
        GUI::new_grid(GUIIdList::DetailTab, 2, egui::Vec2::new(10.0, 5.0))
            .striped(true)
            .max_col_width(ui.available_size().x / 2.0)
            .show(ui, |ui| {
                // 結果を取得する限界数
                ui.label(fl!(LANG_LOADER().get(), "result_max"));
                if egui::DragValue::new(&mut GUI_CONFIG().get_mut().result_max)
                    .clamp_range(1..=1000)
                    .speed(0.5)
                    .ui(ui).changed()
                {
                    SMASHBROS_ENGINE().get_mut().change_result_max();
                }
                ui.end_row();

                // BGM で無効にした時の音量, デバイス, プロセス名
                ui.label(&format!( "{} {}", fl!(LANG_LOADER().get(), "disable"), fl!(LANG_LOADER().get(), "volume") ));
                egui::DragValue::new(&mut GUI_CONFIG().get_mut().gui_state_config.disable_volume)
                    .clamp_range(0.0..=1.0)
                    .speed(0.01)
                    .ui(ui);
                ui.end_row();

                let now_device_name = GUI_CONFIG().get_mut().bgm_device_name.clone().unwrap_or(fl!(LANG_LOADER().get(), "empty"));
                egui::ComboBox::from_id_source(GUIIdList::BgmDeviceComboBox)
                    .selected_text(Self::get_small_caption( now_device_name.clone(), 10 ))
                    .show_ui(ui, |ui| {
                        for (device_name, _) in &self.bgm_device_list {
                            if ui.add(egui::SelectableLabel::new(&now_device_name == device_name, device_name)).clicked() {
                                GUI_CONFIG().get_mut().bgm_device_name = Some(device_name.clone());
                            }
                        }
                    });
                let now_session_name = GUI_CONFIG().get_mut().bgm_session_name.clone().unwrap_or(fl!(LANG_LOADER().get(), "empty"));
                egui::ComboBox::from_id_source(GUIIdList::BgmSessionComboBox)
                    .selected_text(Self::get_small_caption( now_session_name.clone(), 10 ))
                    .show_ui(ui, |ui| {
                        for (session_name, _) in &self.bgm_device_list[&now_device_name] {
                            if ui.add(egui::SelectableLabel::new(&now_session_name == session_name, session_name)).clicked() {
                                GUI_CONFIG().get_mut().bgm_session_name = Some(session_name.clone());
                            }
                        }
                    });
                ui.end_row();
                
                // 代わりに再生する BGM リストフォルダ
                ui.label(fl!(LANG_LOADER().get(), "play_list"));
                ui.add(
                    egui::TextEdit::singleline(&mut GUI_CONFIG().get_mut().bgm_playlist_folder)
                        .hint_text(fl!(LANG_LOADER().get(), "folder"))
                );
                if !ui.ctx().input().raw.dropped_files.is_empty() {
                    if let Some(path_buf) = ui.ctx().input().raw.dropped_files[0].path.clone() {
                        if path_buf.is_dir() {
                            GUI_CONFIG().get_mut().bgm_playlist_folder = path_buf.to_string_lossy().to_string();
                        }
                    }
                }
                ui.end_row();

                // BGM リスト音量
                ui.label(&format!( "{} {}", fl!(LANG_LOADER().get(), "play_list"), fl!(LANG_LOADER().get(), "volume") ));
                ui.add_enabled_ui(!SOUND_MANAGER().get().is_playing(Some(SoundType::Beep)), |ui| {
                    egui::DragValue::new(&mut GUI_CONFIG().get_mut().gui_state_config.play_list_volume)
                        .clamp_range(0.0..=1.0)
                        .speed(0.01)
                        .ui(ui);
                    if ui.button(fl!(LANG_LOADER().get(), "play")).clicked() {
                        SOUND_MANAGER().get_mut().set_volume(GUI_CONFIG().get_mut().gui_state_config.play_list_volume);
                        SOUND_MANAGER().get_mut().beep(440.0, std::time::Duration::from_millis(500));
                    }
                });
                ui.end_row();

                // ストック警告
                ui.label(&format!( "{} {}", fl!(LANG_LOADER().get(), "stock"), fl!(LANG_LOADER().get(), "warning") ));
                ui.scope(|ui| {
                    egui::DragValue::new(&mut GUI_CONFIG().get_mut().gui_state_config.stock_warning_under)
                        .clamp_range(2..=4)
                        .ui(ui);
                    ui.add(
                        egui::TextEdit::singleline(&mut GUI_CONFIG().get_mut().stock_alert_command)
                            .hint_text("command")
                    );
                });
                if !ui.ctx().input().raw.dropped_files.is_empty() {
                    if let Some(path_buf) = ui.ctx().input().raw.dropped_files[0].path.clone() {
                        if path_buf.is_file() {
                            GUI_CONFIG().get_mut().stock_alert_command = path_buf.to_string_lossy().to_string();
                        }
                    }
                }
            });
    }

    // カスタマイズの設定の view を返す
    fn customize_settings_view(&mut self, ui: &mut egui::Ui) {
        GUI::new_grid(GUIIdList::CustomizeTab, 3, egui::Vec2::new(0.0, 5.0))
            .striped(true)
            .show(ui, |ui| {
                ui.checkbox(&mut GUI_CONFIG().get_mut().gui_state_config.chara_image, fl!(LANG_LOADER().get(), "chara_image"));
                ui.checkbox(&mut GUI_CONFIG().get_mut().gui_state_config.win_rate, fl!(LANG_LOADER().get(), "win_rate"));
                ui.checkbox(&mut GUI_CONFIG().get_mut().gui_state_config.wins, fl!(LANG_LOADER().get(), "wins"));
                ui.end_row();

                ui.checkbox(&mut GUI_CONFIG().get_mut().gui_state_config.win_lose, fl!(LANG_LOADER().get(), "win_lose"));
                ui.checkbox(&mut GUI_CONFIG().get_mut().gui_state_config.graph, fl!(LANG_LOADER().get(), "graph"));
                ui.checkbox(&mut GUI_CONFIG().get_mut().gui_state_config.gsp, fl!(LANG_LOADER().get(), "gsp"));
                ui.end_row();

                ui.checkbox(&mut GUI_CONFIG().get_mut().gui_state_config.battling, fl!(LANG_LOADER().get(), "battling"));
                ui.end_row();
            });
    }
}
impl GUIModelTrait for WindowConfiguration {
    fn name(&self) -> String { fl!(LANG_LOADER().get(), "config") }
    fn show(&mut self, ctx: &egui::CtxRef) {
        egui::Window::new( self.name() )
            .default_rect(Self::get_initial_window_rect())
            .vscroll(true)
            .show(ctx, |ui| self.ui(ui));
    }
    fn setup(&mut self, ctx: &egui::CtxRef) {
        if let Some(visuals) = GUI_CONFIG().get_mut().visuals.as_ref() {
            ctx.set_visuals(visuals.clone());
        }
        self.video_device_id = -1;
        self.font_family_list = font_kit::source::SystemSource::new().all_families().unwrap();
        match SOUND_MANAGER().get_mut().load(GUI_CONFIG().get_mut().bgm_playlist_folder.clone()) {
            Ok(_) => (),
            Err(err) => log::error!("{}", err),
        }
    }
}
impl GUIViewTrait for WindowConfiguration {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.config_tab, ConfigTab::Source, fl!(LANG_LOADER().get(), "tab_source"));
            ui.selectable_value(&mut self.config_tab, ConfigTab::Appearance, fl!(LANG_LOADER().get(), "tab_appearance"));
            ui.selectable_value(&mut self.config_tab, ConfigTab::Detail, fl!(LANG_LOADER().get(), "tab_detail"));
            ui.selectable_value(&mut self.config_tab, ConfigTab::Customize, fl!(LANG_LOADER().get(), "tab_customize"));
        });
        ui.separator();

        match self.config_tab {
            ConfigTab::Source => self.source_settings_view(ui),
            ConfigTab::Appearance => self.appearance_settings_view(ui),
            ConfigTab::Detail => self.detail_settings_view(ui),
            ConfigTab::Customize => self.customize_settings_view(ui),
        }

        ui.allocate_space(ui.available_size());
    }
}

// 対戦情報グループ
#[derive(Clone, Default)]
struct WindowBattleInformationGroup {
    data: Option<SmashbrosData>,
}
impl WindowBattleInformationGroup {
    // BattleInformationGroup を表示するのに必要なデータを設定する
    fn set_data(&mut self, data: SmashbrosData) {
        self.data = Some(data);
    }

    // BattleInformationGroup から 戦歴情報の削除を試みる
    fn delete_data(&mut self) -> bool {
        if self.data.is_none() {
            return false;
        }

        if BATTLE_HISTORY().get_mut().delete_data(self.data.as_ref().unwrap()).is_ok() {
            log::info!("delete battle data: {:?}", self.data);
            self.data = None;

            true
        } else {
            false
        }
    }

    // WindowBattleInformationGroup を削除ボタン付きで一覧表示する
    pub fn show_group_list_with_delete(ui: &mut egui::Ui, group_list: &mut Vec<WindowBattleInformationGroup>) -> bool {
        let mut remove_index = None;
        let len = group_list.len();

        for (index, group) in group_list.iter_mut().enumerate() {
            group.show_ui(ui, |ui: &mut egui::Ui| {
                // 戦歴一覧だけ削除できるボタンを設置する
                ui.add_space(16.0);
                ui.add(egui::Separator::default().vertical());
                if ui.add(egui::Button::new("❌🗑").fill(egui::Color32::RED)).clicked() {
                    remove_index = Some(index);
                }
            });

            if index < len - 1 {
                ui.separator();
            }
        };

        if let Some(index) = remove_index {
            let mut group = group_list.remove(index);
            if !group.delete_data() {
                group_list.insert(index, group);

                false
            } else {
                true
            }
        } else {
            false
        }
    }

    // キャラと順位の表示
    fn show_player_chara(ui: &mut egui::Ui, data: &mut SmashbrosData, player_id: i32) {
        let button = if let Some(order_texture) = SMASHBROS_RESOURCE().get_mut().get_order_handle(data.get_order(player_id)) {
            let size = SMASHBROS_RESOURCE().get_mut().get_image_size(order_texture).unwrap();
            egui::Button::image_and_text(order_texture, size * egui::Vec2::new(0.25, 0.25), "")
        } else {
            egui::Button::new("?")
        };

        if let Some(chara_image) = GUI::get_chara_image(data.as_ref().get_character(player_id), [32.0, 32.0]) {
            ui.add_sized( [32.0, 32.0], chara_image);
        } else {
            ui.add_sized( [32.0, 32.0], egui::Label::new(format!("{}p", player_id + 1)) );
        }
        egui::Grid::new(GUIIdList::BattleInformationChildGrid)
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

    // ルールの表示
    fn show_rule(ui: &mut egui::Ui, data: &mut SmashbrosData) {
        use crate::data::BattleRule;

        let max_minute = data.get_max_time().as_secs() / 60;
        let max_minute = format!(
            "{:1}",
            if max_minute == 0 { "?".to_string() } else { max_minute.to_string() }
        );
        let max_stock = data.get_max_stock(0);
        let max_stock = format!(
            "{}",
            if max_stock == -1 { "?".to_string() } else { max_stock.to_string() }
        );

        match data.get_rule() {
            BattleRule::Time => {
                let max_second = data.get_max_time().as_secs() % 60;
                let max_second = format!(
                    "{:02}",
                    if max_minute == "?" { "?".to_string() } else { max_second.to_string() }
                );
        
                ui.add_sized( [16.0, 16.0], egui::Label::new("⏱") );
                ui.add_sized( [0.0, 0.0], egui::Label::new("") );
                ui.end_row();
                ui.add_sized( [16.0, 16.0], egui::Label::new(max_minute + ":") );
                ui.add_sized( [16.0, 16.0], egui::Label::new(max_second) );
                ui.end_row();
            },
            BattleRule::Stock => {
                ui.add_sized( [16.0, 16.0], egui::Label::new("⏱") );
                ui.add_sized( [16.0, 16.0], egui::Label::new(max_minute) );
                ui.end_row();
                ui.add_sized( [16.0, 16.0], egui::Label::new("👥") );
                ui.add_sized( [16.0, 16.0], egui::Label::new(max_stock));
                ui.end_row();
            },
            BattleRule::Stamina => {
                let max_hp = data.get_max_hp(0);
                let max_hundreds = format!(
                    "{}",
                    if max_hp == 0 { "?".to_string() } else { (max_hp / 100).to_string() }
                );
                let max_tens = format!(
                    "{}",
                    if max_hp == 0 { "?".to_string() } else { (max_hp % 100).to_string() }
                );
        
                ui.add_sized( [16.0, 16.0], egui::Label::new("⏱".to_string() + &max_minute) );
                ui.add_sized( [16.0, 16.0], egui::Label::new("👥".to_string() + &max_stock) );
                ui.end_row();
                ui.add_sized( [16.0, 16.0], egui::Label::new("💖".to_string() + &max_hundreds));
                ui.add_sized( [16.0, 16.0], egui::Label::new(max_tens));
                ui.end_row();
            },
            BattleRule::Tournament => {
                ui.add_sized( [16.0, 16.0], egui::Label::new("🏆") );
                ui.add_sized( [16.0, 16.0], egui::Label::new("") );
                ui.end_row();
            },
            _ => {
                ui.add_sized( [16.0, 16.0], egui::Label::new("?") );
                ui.add_sized( [16.0, 16.0], egui::Label::new("") );
                ui.end_row();
            },
        }

        // スタミナ の表示
        match data.get_rule() {
            BattleRule::Stamina => {
            },
            _ => (),
        }
    }

    fn show_ui(&mut self, ui: &mut egui::Ui, add_ui: impl FnOnce(&mut egui::Ui)) {
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

                // ルールの表示
                GUI::new_grid("rules_icons", 2, egui::Vec2::new(0.0, 0.0))
                    .show(ui, |ui| {
                        Self::show_rule(ui, data);
                    });
                ui.add(egui::Separator::default().vertical());

                // ストックの表示
                GUI::new_grid("stocks_icons", 3, egui::Vec2::new(0.0, 0.0))
                    .show(ui, |ui| {
                        Self::show_player_stock(ui, data, 0);
                        ui.end_row();
                        Self::show_player_stock(ui, data, 1);
                    });

                // 追加の UI の表示
                add_ui(ui);
                ui.end_row();
            });
    }
}

#[derive(PartialEq)]
enum WinsGraphKind {
    Gsp,
    Rate,
}
impl Default for WinsGraphKind {
    fn default() -> Self {
        WinsGraphKind::Gsp
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
    wins: i32,
    kind: WinsGraphKind,
    border_line_list: Vec<Vec::<plot::Value>>,
}
impl WindowWinsGraph {
    fn set_data(&mut self, data: SmashbrosData, data_list: Vec<SmashbrosData>, wins_lose: (i32, i32), win_rate: (f32, i32), kind: WinsGraphKind) {
        self.now_data = Some(data);
        self.wins_lose = wins_lose;
        self.win_rate = win_rate;
        self.kind = kind;

        let mut data_list = data_list;
        data_list.reverse();
        let last = match data_list.last() {
            Some(last) => &last,
            None => self.now_data.as_ref().unwrap(),
        };
        self.last_power = last.get_power(0);

        // グラフにデータを追加
        let mut battle_count = 0.0;
        let mut rate = 0.0;
        let mut upper_power = 0;
        let mut prev_power_list = Vec::new();
        let mut prev_chara_list = Vec::new();
        self.wins = 0;
        self.point_list = data_list.iter().enumerate().filter_map(|(x, data)| {
            // 連勝記録
            if let Some(is_win) = data.is_win() {
                if is_win {
                    self.wins += 1;
                } else {
                    self.wins = 0;
                }
            } else {
                self.wins = 0;
            }

            // WinsGraphKind によってグラフの内容を変える
            match self.kind {
                WinsGraphKind::Gsp => {
                    if let Some(is_valid_power) = data.is_valid_power(0, data.get_power(0), Some(&prev_power_list), Some(&prev_chara_list), false) {
                        if !is_valid_power {
                            return None;
                        }
                    } else {
                        return None;
                    }
                    prev_power_list = vec![data.get_power(0), data.get_power(1)];
                    prev_chara_list = vec![data.get_character(0), data.get_character(1)];

                    if upper_power < data.get_power(0) {
                        upper_power = data.get_power(0);
                    }
        
                    battle_count = x as f64;
                    Some(plot::Value::new(battle_count, data.get_power(0) as f64))
                },
                WinsGraphKind::Rate => {
                    if let Some(is_win) = data.is_win() {
                        if is_win {
                            rate += 1.0;
                        }
                        battle_count += 1.0;
                    } else {
                        return None;
                    }

                    Some(plot::Value::new(battle_count, rate / battle_count * 100.0))
                },
            }
        }).collect::<Vec<plot::Value>>();

        // グラフへ基準点の作成
        match self.kind {
            WinsGraphKind::Gsp => {
                // 100万の区切りの上下を表示
                let upper_one_mil = upper_power / 1_000_000 + 1;
                self.border_line_list = vec![
                    vec![
                        plot::Value::new(0.0, ((upper_one_mil - 2) * 1_000_000) as f64),
                        plot::Value::new(battle_count, ((upper_one_mil - 2) * 1_000_000) as f64)
                    ],
                    vec![
                        plot::Value::new(0.0, ((upper_one_mil - 1) * 1_000_000) as f64),
                        plot::Value::new(battle_count, ((upper_one_mil - 1) * 1_000_000) as f64)
                    ],
                    vec![
                        plot::Value::new(0.0, (upper_one_mil * 1_000_000) as f64),
                        plot::Value::new(battle_count, (upper_one_mil * 1_000_000) as f64)
                    ]
                ];
            },
            WinsGraphKind::Rate => {
                // 0%, 100%
                self.border_line_list = vec![
                    vec![
                        plot::Value::new(0.0, 0.0),
                        plot::Value::new(battle_count, 0.0),
                    ],
                    vec![
                        plot::Value::new(0.0, 50.0),
                        plot::Value::new(battle_count, 50.0),
                    ],
                    vec![
                        plot::Value::new(0.0, 100.0),
                        plot::Value::new(battle_count, 100.0),
                    ]
                ];
            },
        }
    }

    fn show_graph(&self, ui: &mut egui::Ui, plot_name: &String) {
        let theme_color = if ui.ctx().style().visuals == Visuals::dark() {
            egui::Color32::RED
        } else {
            egui::Color32::WHITE
        };
        let theme_gray_color = if ui.ctx().style().visuals == Visuals::dark() {
            egui::Color32::GRAY
        } else {
            egui::Color32::WHITE
        };
        plot::Plot::new(GUIIdList::PowerPlot)
            .width(GUI::get_initial_window_size().x / 2.0)
            .height(40.0)
            .legend(plot::Legend::default())
            .view_aspect(1.0)
            .show_axes([false, false])
            .show(ui, |ui| {
                ui.line(
                    plot::Line::new(plot::Values::from_values( self.point_list.clone() ))
                        .color(theme_color)
                );
                if !self.border_line_list.is_empty() {
                    for (i, border_line_list) in self.border_line_list.iter().enumerate() {
                        ui.line(
                            plot::Line::new(plot::Values::from_values(border_line_list.clone()))
                                .color(if i == 1 { theme_gray_color } else { egui::Color32::WHITE })
                                .style(plot::LineStyle::dashed_dense())
                        );
                    }
                }
                ui.points(
                    plot::Points::new(plot::Values::from_values( self.point_list.clone() ))
                        .radius(2.0)
                        // Light モードのときだけ点を白にすることで、GSP だけをクリッピングして表示しやすいようにする
                        .color(theme_color)
                        .name(format!("{}\n{}", plot_name, match self.kind {
                            WinsGraphKind::Gsp => format!("{}", if -1 == self.last_power {
                                format!("{}", fl!(LANG_LOADER().get(), "empty"))
                            } else {
                                format!("{}", self.last_power)
                            }),
                            WinsGraphKind::Rate => format!("o:{}/x:{}", self.wins_lose.0, self.wins_lose.1),
                        }))
                );
            });
    }

    fn show_wins_group(&self, ui: &mut egui::Ui, available_size: egui::Vec2) {
        GUI::new_grid("wins_group", 2, egui::Vec2::new(5.0, 0.0))
        // .min_col_width(0.0)
        .min_col_width(available_size.x / 5.0)
        .show(ui, |ui| {
            let now_data = match &self.now_data {
                Some(data) => data,
                None => return,
            };
            if !now_data.is_decided_character_name(0) || !now_data.is_decided_character_name(1) {
                return;
            }

            // キャラ画像
            if GUI_CONFIG().get_mut().gui_state_config.chara_image {
                ui.scope(|ui| {
                    if let Some(image) = GUI::get_chara_image(now_data.as_ref().get_character(0), [16.0, 16.0]) {
                        ui.add(image);
                    } else {
                        ui.small("1p");
                    }
                    ui.small("x");
                    if let Some(image) = GUI::get_chara_image(now_data.as_ref().get_character(1), [16.0, 16.0]) {
                        ui.add(image);
                    } else {
                        ui.small("2p");
                    }
                });
            }
            // 勝率表示
            if GUI_CONFIG().get_mut().gui_state_config.win_rate {
                ui.scope(|ui| {
                    match self.kind {
                        WinsGraphKind::Gsp => {
                            ui.small(format!("{:3.1}%({})", 100.0 * self.win_rate.0, self.win_rate.1));
                        },
                        WinsGraphKind::Rate => {
                            ui.add(egui::Separator::default().vertical());
                            ui.small(format!("{:3.1}%", 100.0 * self.win_rate.0));
                        },
                    }
                });
            }
            ui.end_row();

            if self.kind == WinsGraphKind::Gsp {
                ui.end_row();
            }

            // 連勝表示
            if GUI_CONFIG().get_mut().gui_state_config.wins {
                ui.scope(|ui| {
                    ui.small(format!( "{}", self.wins ));
                    ui.small(format!( "{}", fl!(LANG_LOADER().get(), "wins") ));
                });
            }
            if GUI_CONFIG().get_mut().gui_state_config.win_lose {
                ui.scope(|ui| {
                    match self.kind {
                        WinsGraphKind::Gsp => {
                            // 勝敗数表示
                            ui.small(format!( "o:{}/x:{}", self.wins_lose.0, self.wins_lose.1 ));
                        },
                        WinsGraphKind::Rate => {
                            // 試合数表示
                            ui.add(egui::Separator::default().vertical());
                            ui.small(format!("({})", self.win_rate.1));
                        },
                    }
                });
            }
            ui.end_row();
        });
    }

    const MAX_FONT_WIDTH: i32 = 32;
    fn show_ui(&self, ui: &mut egui::Ui, plot_name: String) {
        let available_size = ui.available_size();
        GUI::new_grid("wins_graph_group", 2, egui::Vec2::new(0.0, 0.0))
            .min_col_width(120.0)
            .show(ui, |ui| {
                if GUI_CONFIG().get_mut().gui_state_config.is_show_wins_group() {
                    self.show_wins_group(ui, available_size);
                }

                // 世界戦闘力グラフの表示
                if GUI_CONFIG().get_mut().gui_state_config.gsp {
                    let font_size = GUI_CONFIG().get_mut().font_size.unwrap_or(16);
                    if Self::MAX_FONT_WIDTH < font_size {
                        // 見切れる場合は戦闘力を100万単位にする
                        let gsp = self.last_power as f32 / 10_000.0;
                        ui.scope(|ui| {
                            let gsp_string = if -1 == self.last_power { fl!(LANG_LOADER().get(), "empty") } else { format!( "{:.0}", gsp ) };
                            ui.small(gsp_string);
                            ui.small(format!( "{}", fl!(LANG_LOADER().get(), "million") ));
                        });
                    } else {
                        ui.scope(|ui| {
                            let gsp_string = if -1 == self.last_power { fl!(LANG_LOADER().get(), "empty") } else { format!( "{}", self.last_power ) };
                            ui.small(format!( "{}", gsp_string ));
                            ui.small(format!( "{}", fl!(LANG_LOADER().get(), "gsp") ));
                        });
                    }
                } else if GUI_CONFIG().get_mut().gui_state_config.graph {
                    self.show_graph(ui, &plot_name);
                }
            });
    }
}

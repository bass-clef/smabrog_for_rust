
use anyhow::*;
use iced::{
    pane_grid, scrollable,
    Button, Column, Command, Container, Element,
    HorizontalAlignment, Length,
    PaneGrid, PickList, Scrollable, Text, TextInput,
    VerticalAlignment,
};
use serde::{
    Deserialize,
    Serialize
};
use std::time::{Duration, Instant};


use crate::capture::CaptureFromVideoDevice;
use crate::data::{
    SmashbrosData, SmashbrosDataTrait,
    SMASHBROS_RESOURCE
};
use crate::engine::*;
use crate::scene::SceneList;


#[derive(Debug, Clone)]
pub enum Message {
    None,
    CaptureModeChanged(CaptureMode),
    CaptureDeviceChanged(String),
    DummyMessage,
    InputWindowCaption(String),
    InputWindowClass(String),
    SettingsApply,
    Tick(Instant),
    TitleClicked(pane_grid::Pane),
    TileClicked(pane_grid::Pane),
    WindowCloseRequest,
}

/* GUIを管理するクラス */
pub struct GUI {
    engine: SmashBrogEngine,
    count: i32,
    started_window: bool,
    should_exit: bool,
    gui_config: GUIConfig,
    pane_battle_infomation: pane_grid::State<ContentBattleInfomation>,
    pane_battle_history: pane_grid::State<ContentBattleHistory>,
    pane_settings: pane_grid::State<ContentSettings>,
    selected_capture_mode: CaptureMode,
}
impl Default for GUI {
    fn default() -> Self {
        unsafe{ CAPTION.set(String::from(Self::DEFAULT_CAPTION)).unwrap() };

        Self {
            engine: Default::default(),
            count: 0,
            started_window: false,
            should_exit: false,
            gui_config: Default::default(),
            pane_battle_infomation: pane_grid::State::new(ContentBattleInfomation::new()).0,
            pane_battle_history: pane_grid::State::new(ContentBattleHistory::new()).0,
            pane_settings: pane_grid::State::new(ContentSettings::new()).0,
            selected_capture_mode: CaptureMode::default(),
        }
    }
}
// iced_winit が application::Application を要求してくるのでもろもろ定義(iced::Applicationを分割しただけ)
// こちらは iced_native によるイベントの処理などが書かれている
impl iced_winit::application::Application for GUI {
    type Flags = ();

    fn new(_: Self::Flags) -> (Self, Command<Self::Message>) {
        (
            Default::default(),
            Command::none()
        )
    }
    fn title(&self) -> String { Self::get_title().to_string() }

    // ここで iced と winit の Event を相互変換したり、独自の Event を発行する
    fn subscription(&self) -> iced::Subscription<Message> {
        let mut subscription_list = vec![];

        subscription_list.push(iced_native::subscription::events_with(|event, status| {
            if let iced_winit::event::Status::Captured = status {
                return None;
            }
            match event {
                iced_native::Event::Window(event) => {
                    match event {
                        iced_native::window::Event::CloseRequested => {
                            Some(Message::WindowCloseRequest)
                        },
                        _ => None,
                    }
                },
                _ => None,
            }
        }));

        // iced でタイマー処理するには、非同期にイベントを発行しなければいけないらしい
        subscription_list.push(
            iced::time::every(Duration::from_millis(1000/15))
                .map(|instant| Message::Tick(instant))
        );

        iced::Subscription::batch(subscription_list)
    }

    fn should_exit(&self) -> bool {
        self.should_exit
    }
}

// iced_winit が Program を要求してくるのでもろもろ定義(iced::Applicationを分割しただけ)
// こちらは iced_wpgu によるレンダリングに関するものが書かれてる
impl iced_winit::Program for GUI {
    type Renderer = iced_wgpu::Renderer;
    type Message = Message;
    type Clipboard = iced::Clipboard;

    // subscription で予約発行されたイベントの処理
    fn update(&mut self, message: Message, _clipboard: &mut Self::Clipboard) -> Command<Message> {
        match message {
            Message::CaptureModeChanged(capture_mode) => {
                self.selected_capture_mode = capture_mode;
                self.load_config(false).unwrap_or(());
            },
            Message::CaptureDeviceChanged(device_name) => {
                if let CaptureMode::VideoDevice(_, _, dev_name) = self.selected_capture_mode.as_mut() {
                    *dev_name = device_name.clone();
                }
            },
            Message::DummyMessage => self.count = 0,
            Message::InputWindowCaption(window_caption) => {
                if let CaptureMode::Window(_, win_caption, _) = self.selected_capture_mode.as_mut() {
                    *win_caption = window_caption.clone();
                }
            },
            Message::InputWindowClass(window_class) => {
                if let CaptureMode::Window(_, _, win_class) = self.selected_capture_mode.as_mut() {
                    *win_class = window_class.clone();
                }
            },
            Message::SettingsApply => {
                // USB の ID は直前で書き換え
                if let CaptureMode::VideoDevice(_, dev_id, dev_name) = self.selected_capture_mode.as_mut() {
                    if let Some(device_list) = CaptureFromVideoDevice::get_device_list() {
                        if let Some(device_id) = device_list.get(dev_name) {
                            *dev_id = *device_id;
                        }
                    }
                }

                self.save_config(false).unwrap_or(());

                if let Err(e) = self.engine.change_capture_mode(self.selected_capture_mode.as_ref()) {

                }
            },
            Message::Tick(_) => {
                self.count += 1;
                match self.engine.update() {
                    Ok(_) => {
                        // コンフィグを読み込む時にウィンドウのもろもろもあるので、
                        // Tick で何回か window の存在確認をする
                        if false == self.started_window {
                            if self.load_config(true).is_ok() {
                                self.started_window = true;
                            }
                        }
                    },
                    Err(e) => {
                        // quit
                        log::error!("quit. [{}]", e);
                        self.should_exit = true;
                    }
                }
            },
            Message::WindowCloseRequest => {
                self.save_config(true).unwrap_or(());
                self.should_exit = true;
            },
            _ => (),
        }

        Command::none()
    }

    // ui の表示(静的でなく動的で書き換えられる)
    fn view(&mut self) -> Element<Message> {
        let now_data = self.engine.get_now_data();
        let pane_battle_infomation = PaneGrid::new(&mut self.pane_battle_infomation, |pane, content| {
                let title_bar = pane_grid::TitleBar::new(Text::new("Battle Infomation:"))
                    .padding(10)
                    .style(style::TitleBar{ pane_type: style::PaneType::Information });

                pane_grid::Content::new( content.view(pane, now_data.clone()) )
                    .title_bar(title_bar)
                    .style(style::Pane{ pane_type: style::PaneType::Information })
            })
            .width(Length::Fill)
            .height(Length::FillPortion(16))
            .on_click(Message::TitleClicked);

        let data_list = self.engine.get_data_latest_10();
        let pane_battle_history = PaneGrid::new(&mut self.pane_battle_history, |pane, content| {
                let title_bar = pane_grid::TitleBar::new(Text::new("Battle History:"))
                    .padding(10)
                    .style(style::TitleBar{ pane_type: style::PaneType::History });

                pane_grid::Content::new( content.view(pane, data_list.clone()) )
                    .title_bar(title_bar)
                    .style(style::Pane{ pane_type: style::PaneType::History })
            })
            .width(Length::Fill)
            .height(Length::FillPortion(64))
            .on_click(Message::TitleClicked);

        let selected_capture_mode = self.selected_capture_mode.as_ref();
        let captured_scene = self.engine.get_captured_scene();
        let pane_settings = PaneGrid::new(&mut self.pane_settings, |pane, content| {
                let (view, title_bar) = content.view(pane, selected_capture_mode, captured_scene);
                pane_grid::Content::new(view)
                    .title_bar(title_bar)
                    .style(style::Pane{ pane_type: style::PaneType::Settings })
            })
            .width(Length::Fill)
            .height(Length::FillPortion(20))
            .on_click(Message::TitleClicked);

        Column::new()
            .align_items(iced::Align::Start)
            .push(pane_battle_infomation)
            .push(pane_battle_history)
            .push(pane_settings)
            .into()
    }
}
use once_cell::sync::OnceCell;
static mut CAPTION: OnceCell<String> = OnceCell::new();
impl GUI {
    const DEFAULT_CAPTION: &'static str = "smabrog";
    const CONFIG_FILE: &'static str = "config.json";

    /// 設定情報の読み込み
    fn load_config(&mut self, is_initalize: bool) -> Result<()> {
        let file = std::fs::File::open(Self::CONFIG_FILE)?;
        self.gui_config = serde_json::from_reader(std::io::BufReader::new(file))?;

        if is_initalize && cfg!(target_os = "windows") {
            unsafe {
                // 位置復元
                use winapi::um::winuser;
                use crate::utils::utils::to_wchar;
                use winapi::shared::minwindef::BOOL;
                let own_handle = winuser::FindWindowW(std::ptr::null_mut(), to_wchar(Self::DEFAULT_CAPTION));
                if own_handle.is_null() {
                    return Err(anyhow!("Not found Window."));
                }
                // リサイズされるのを期待して適当に大きくする
                winuser::MoveWindow(own_handle, self.gui_config.window_x, self.gui_config.window_y, 256+16, 720+39, true as BOOL);
            }
            log::info!("loaded config.");
        }

        if let CaptureMode::Window(_, win_caption, win_class) = self.selected_capture_mode.as_mut() {
            *win_caption = self.gui_config.capture_win_caption.clone();
            *win_class = self.gui_config.capture_win_class.clone();
        } else if let CaptureMode::Window(_, _, device_name) = self.selected_capture_mode.as_mut() {
            *device_name = self.gui_config.capture_device_name.clone();
        }

        Ok(())
    }
    /// 設定情報の保存
    fn save_config(&mut self, is_finalize: bool) -> Result<(), Box<dyn std::error::Error>> {
        if is_finalize && cfg!(target_os = "windows") {
            unsafe {
                // 位置復元用
                use winapi::um::winuser;
                use crate::utils::utils::to_wchar;
    
                let own_handle = winuser::FindWindowW(std::ptr::null_mut(), to_wchar(Self::get_title()));
                if !own_handle.is_null() {
                    let mut window_rect = winapi::shared::windef::RECT { left:0, top:0, right:0, bottom:0 };
                    winapi::um::winuser::GetWindowRect(own_handle, &mut window_rect);
                    self.gui_config.window_x = window_rect.left;
                    self.gui_config.window_y = window_rect.top;
                }
            }
            log::info!("saved config.");
        }

        match self.selected_capture_mode.as_ref() {
            CaptureMode::VideoDevice(_, _, device_name) => {
                self.gui_config.capture_device_name = device_name.clone();
            },
            CaptureMode::Window(_, win_caption, win_class) => {
                self.gui_config.capture_win_caption = win_caption.clone();
                self.gui_config.capture_win_class = win_class.clone();
            },
            _ => (),
        }

        use std::io::Write;
        let serialized_data = serde_json::to_string(&self.gui_config).unwrap();
        let mut file = std::fs::File::create(Self::CONFIG_FILE)?;
        file.write_all(serialized_data.as_bytes())?;

        Ok(())
    }

    // 他モジュールから動的にキャプションを変更するためのもの
    pub fn get_title() -> &'static str {
        unsafe {
            match CAPTION.get() {
                Some(string) => {
                    &string
                },
                None => "",
            }
        }
    }

    pub fn set_title(new_caption: &str) {
        unsafe {
            let caption = CAPTION.get_mut();
            match caption {
                Some(string) => {
                    string.clear();
                    string.push_str(new_caption);
                },
                None => (),
            };
        }
    }
}

// 設定ファイル
#[derive(Debug, Default, Deserialize, Serialize)]
struct GUIConfig {
    pub window_x: i32,
    pub window_y: i32,
    pub capture_win_caption: String,
    pub capture_win_class: String,
    pub capture_device_name: String,
}


// 対戦中情報
struct ContentBattleInfomation {
    pane_now_battle_tile: pane_grid::State<ContentBattleTile>,
}
impl ContentBattleInfomation {
    fn new() -> Self {
        Self {
            pane_now_battle_tile: pane_grid::State::new(ContentBattleTile::default()).0
        }
    }

    fn view(&mut self, _pane: pane_grid::Pane, data: SmashbrosData) -> Element<Message> {
        use std::sync::{ Arc, Mutex };
        let data = Arc::new(Mutex::new( data ));
        let pane_now_battle_tile = PaneGrid::new(&mut self.pane_now_battle_tile, |pane, content|{
                let data = data.clone();
                let data: SmashbrosData = (*data.lock().unwrap()).clone();

                pane_grid::Content::new(content.view(pane, data))
                    .style(style::Pane{ pane_type: style::PaneType::Tile })
            })
            .width(Length::Fill)
            .height(Length::from(32))
            .on_click(Message::TileClicked);

        let controlls = Column::new()
            .push(pane_now_battle_tile);

        Container::new(controlls)
            .padding(5)
            .into()
    }
}

// 戦歴
struct ContentBattleHistory {
    scroll: scrollable::State,
    pane_battle_tile_list: Vec< (pane_grid::State<ContentBattleTile>, SmashbrosData) >,
}
impl ContentBattleHistory {
    fn new() -> Self {
        Self {
            scroll: Default::default(),
            pane_battle_tile_list: Vec::new(),
        }
    }

    fn view(&mut self, _pane: pane_grid::Pane, data_list: Vec<SmashbrosData>) -> Element<Message> {
        let mut scrollable = Scrollable::new(&mut self.scroll)
            .spacing(5)
            .width(Length::Fill)
            .height(Length::Fill);

        self.pane_battle_tile_list.clear();
        for data in data_list {
            self.pane_battle_tile_list.push(
                (
                    pane_grid::State::new(ContentBattleTile::default()).0,
                    data
                )
            );
        }
        
        use std::sync::{ Arc, Mutex };
        for (pane_battle_tile, data) in &mut self.pane_battle_tile_list {
            let data = Arc::new(Mutex::new( data ));
            let pane_grid_battle_tile = PaneGrid::new(pane_battle_tile, |pane, content| {
                    let data = data.clone();
                    let data: SmashbrosData = (*data.lock().unwrap()).clone();

                    pane_grid::Content::new(content.view(pane, data))
                        .style(style::Pane{ pane_type: style::PaneType::Tile })
                })
                .width(Length::Fill)
                .height(Length::from(32))
                .on_click(Message::TileClicked);

            scrollable = scrollable.push(pane_grid_battle_tile);
        }

        Container::new(scrollable)
            .padding(5)
            .into()
    }
}

// 設定変更
struct ContentSettings {
    dummy_button: iced::button::State,
    dummy_button_2: iced::button::State,
    apply_button: iced::button::State,
    prev_time: std::time::Instant,

    capture_mode_pick_list: iced::pick_list::State<CaptureMode>,
    device_list_pick_list: iced::pick_list::State<String>,

    capture_mode_all: [CaptureMode; 4],
    device_name_list: Box<[String]>,

    window_caption: iced::text_input::State,
    window_class: iced::text_input::State,
}
impl ContentSettings {
    fn new() -> Self {
        let mut device_name_list: Vec<String> = Vec::new();
        if let Some(devices) = CaptureFromVideoDevice::get_device_list() {
            for (device_name, _) in devices.iter() {
                device_name_list.push(device_name.clone());
            }
        }

        Self {
            dummy_button: iced::button::State::new(),
            dummy_button_2: iced::button::State::new(),
            apply_button: iced::button::State::new(),
            prev_time: std::time::Instant::now(),

            capture_mode_pick_list: Default::default(),
            device_list_pick_list: Default::default(),
            
            device_name_list: device_name_list.into_boxed_slice(),
            capture_mode_all: CaptureMode::ALL.clone(),

            window_caption: iced::text_input::State::focused(),
            window_class: iced::text_input::State::focused(),
        }
    }

    /// 借用書の解決が難しかったので、タプルで content と title_bar を返す
    /// @return (Element<Message>, pane_grid::TitleBar<Message>) (content, title_bar)
    fn view<'b>(&'b mut self, _pane: pane_grid::Pane, capture_mode: &CaptureMode, captured_scene: SceneList) -> (Element<Message>, pane_grid::TitleBar<Message>) {
        // content
        let mut controlls = Column::new();

        let capture_mode_row = iced::Row::new()
            .align_items(iced::Align::Center)
            .spacing(5)
            .push(Text::new("Capture Mode:"))
            .push(PickList::new(
                &mut self.capture_mode_pick_list,
                &self.capture_mode_all[..],
                Some(capture_mode.clone()),
                Message::CaptureModeChanged
            ));
        controlls = controlls.push(capture_mode_row);

        match capture_mode {
            CaptureMode::Window(_, win_caption, win_class) => {
                let capture_window_row = iced::Row::new()
                    .align_items(iced::Align::Center)
                    .push(TextInput::new(
                            &mut self.window_caption,
                            "caption",
                            win_caption,
                            Message::InputWindowCaption
                        )
                    )
                    .push(TextInput::new(
                            &mut self.window_class,
                            "class (can blank)",
                            win_class,
                            Message::InputWindowClass
                        )
                    );

                controlls = controlls.push(capture_window_row);
            },
            CaptureMode::VideoDevice(_, _, device_name) => {
                let capture_video_device_row = iced::Row::new()
                    .align_items(iced::Align::Center)
                    .push(PickList::new(
                        &mut self.device_list_pick_list,
                        &*self.device_name_list,
                        Some(device_name.clone()),
                        Message::CaptureDeviceChanged
                    ));

                controlls = controlls.push(capture_video_device_row);
            }
            _ => ()
        }

        
        controlls = controlls.push(
            Button::new(&mut self.apply_button,
                    Text::new("Apply").horizontal_alignment(HorizontalAlignment::Center)
            )
            .width(Length::Fill)
            .on_press(Message::SettingsApply),
        );

        // title_bar
        let mut title_bar_row = iced::Row::new()
            .align_items(iced::Align::Center)
            .push(Text::new("Settings: [Job: "));

        title_bar_row = match self.prev_time.elapsed().as_millis() as i32 {
            0 ..=  99 => title_bar_row.push(
                Button::new(&mut self.dummy_button, Text::new("Good"))
                    .style(style::ColorButton{ color: style::SUCCESS_COLOR })
                    .on_press(Message::None),
            ),
            100 ..= 999 => title_bar_row.push(
                Button::new(&mut self.dummy_button, Text::new("Uhh."))
                    .style(style::ColorButton{ color: style::INFO_COLOR })
                    .on_press(Message::None),
            ),
            _ => title_bar_row.push(
                Button::new(&mut self.dummy_button, Text::new("Busy"))
                    .style(style::ColorButton{ color: style::WARNING_COLOR })
                    .on_press(Message::None),
            ),
        }.push(Text::new("/"));
        title_bar_row = match &captured_scene {
            SceneList::Unknown => {
                title_bar_row.push(
                    Button::new(&mut self.dummy_button_2, Text::new("NotFound"))
                    .style(style::ColorButton{ color: style::ERROR_COLOR })
                    .on_press(Message::None),
                )
            },
            _ => {
                title_bar_row.push(
                    Button::new(&mut self.dummy_button_2, Text::new(&format!("{:?}", captured_scene)))
                    .style(style::ColorButton{ color: style::SUCCESS_COLOR })
                    .on_press(Message::None),
                )
            }
        }.push(Text::new(" ]"));

        self.prev_time = std::time::Instant::now();

        (
            Container::new(controlls)
                .padding(5)
                .into(),
            pane_grid::TitleBar::new(title_bar_row)
                .padding(10)
                .style(style::TitleBar{ pane_type: style::PaneType::Settings })
        )
    }
}


// 対戦情報
struct ContentBattleTile;
impl Default for ContentBattleTile {
    fn default() -> Self {
        Self {
        }
    }
}
impl ContentBattleTile {
    fn push_chara<'a>(&mut self, row: iced::Row<'a, Message>, chara_name: String, text: &str, order: i32)
        -> iced::Row<'a, Message>
    {
        if let Some(handle) = unsafe{SMASHBROS_RESOURCE.get()}.get_image_handle(chara_name.clone()) {
            let mut row = row.push(iced::image::Image::new(handle));

            if let Some(handle) = unsafe{SMASHBROS_RESOURCE.get()}.get_order_handle(order) {
                row = row.push(
                    iced::Column::new()
                        .push(iced::widget::Space::with_height(Length::FillPortion(1)))
                        .push(
                            iced::image::Image::new(handle)
                                .width(Length::Shrink)
                                .height(Length::FillPortion(1))
                        )
                );
            }

            row
        } else {
            row.push(
                iced::Column::new()
                    .width(Length::from(32))
                    .height(Length::from(32))
                    .align_items(iced::Align::Start)
                    .push(
                        Text::new(text.to_string())
                            .size(14)
                    )
                    .push(
                        Text::new(chara_name)
                            .size(10)
                    )
            )
        }
    }

    fn view(&mut self, _pane: pane_grid::Pane, data: SmashbrosData) -> Element<Message> {
        let mut chara_order_row = iced::Row::new()
            .spacing(5)
            .align_items(iced::Align::Center);

        if 2 != data.get_player_count() {
            chara_order_row = chara_order_row.push(
                Text::new("unknown data.")
                    .width(Length::Fill)
                    .height(Length::from(32))
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .vertical_alignment(VerticalAlignment::Center)
            );
            return Container::new(chara_order_row)
                .style(style::Pane{ pane_type: style::PaneType::Tile })
                .into();
        }
        
        let p1_data_row = self.push_chara(iced::Row::new(), data.get_character(0).clone(), "1p", data.get_order(0));
        let p2_data_row = self.push_chara(iced::Row::new(), data.get_character(1).clone(), "2p", data.get_order(1));
        chara_order_row = chara_order_row
            .push(p1_data_row.width(Length::FillPortion(3)))
            .push(Text::new("vs").width(Length::FillPortion(1)))
            .push(p2_data_row.width(Length::FillPortion(3)));

        let time = data.get_max_time().as_secs();
        let rule_stock_column = iced::Column::new()
            .align_items(iced::Align::Center)
            .push(
                Text::new(format!("Rule: {:?} ({}:{:02})",
                        data.get_rule(),
                        time / 60, time % 60
                    ))
                    .width(Length::Fill)
            )
            .push(
                Text::new(format!("Stock: {} - {} / ({})",
                        data.get_stock(0), data.get_stock(1),
                        if -1 == data.get_max_stock(0) { "??".to_string() } else { data.get_max_stock(0).to_string() },
                    ))
                    .width(Length::Fill)
            );

        let row = iced::Row::new()
            .spacing(5)
            .align_items(iced::Align::Center)
            .push(chara_order_row.width(Length::FillPortion(1)))
            .push(rule_stock_column.width(Length::FillPortion(1)));

        Container::new(row)
            .style(style::Pane{ pane_type: style::PaneType::Tile })
            .into()
    }
}

// 検出する方法
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum CaptureMode {
    Empty(&'static str),
    Desktop(&'static str),
    /// _, device_id
    VideoDevice(&'static str, i32, String),
    /// _, window_caption, window_class
    Window(&'static str, String, String),
}
impl CaptureMode {
    const ALL: [CaptureMode; 4] = [
        Self::Empty { 0:"Not Capture" },
        Self::Desktop { 0:"From Desktop" },
        Self::VideoDevice { 0:"From Video Device", 1:0, 2:String::new() },
        Self::Window { 0:"From Window", 1:String::new(), 2:String::new() },
    ];
}
impl Default for CaptureMode {
    fn default() -> Self {
        Self::ALL[0].clone()
    }
}
impl std::fmt::Display for CaptureMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Empty(show_text) | Self::Desktop(show_text)
                    | Self::VideoDevice(show_text, _, _) | Self::Window(show_text, _, _) => show_text
            }
        )
    }
}
impl AsMut<CaptureMode> for CaptureMode {
    fn as_mut(&mut self) -> &mut CaptureMode { self }
}
impl AsRef<CaptureMode> for CaptureMode {
    fn as_ref(&self) -> &CaptureMode { self }
}


// 外観(色/線)
mod style {
    use iced::{
        Background, Color
    };
    use iced_wgpu::container;
    
    const INFORMATION: Color = Color::from_rgb(200.0 / 255.0, 0.0, 0.0);
    const HISTORY: Color = Color::from_rgb(228.0 / 255.0, 38.0 / 255.0, 111.0 / 255.0);
    const SETTINGS: Color = Color::from_rgb(0.0, 103.0 / 255.0, 221.0 / 255.0);
    const TILE: Color = Color::from_rgb(221.0 / 255.0, 159.0 / 255.0, 0.0);

    const TITLE_TEXT_COLOR: Color = Color::from_rgb(222.0 / 255.0, 222.0 / 255.0, 222.0 / 255.0);

    pub const INFO_COLOR: Color = Color::from_rgb(0xDB as f32 / 255.0, 0xE5 as f32 / 255.0, 0xF8 as f32 / 255.0);
    pub const SUCCESS_COLOR: Color = Color::from_rgb(0xDF as f32 / 255.0, 0xF2 as f32 / 255.0, 0xBF as f32 / 255.0);
    pub const WARNING_COLOR: Color = Color::from_rgb(0xFE as f32 / 255.0, 0xEF as f32 / 255.0, 0xB3 as f32 / 255.0);
    pub const ERROR_COLOR: Color = Color::from_rgb(0xFF as f32 / 255.0, 0xD2 as f32 / 255.0, 0xD2 as f32 / 255.0);

    #[derive(Clone, Copy)]
    pub enum PaneType {
        Information,
        History,
        Settings,
        Tile,
    }
    impl PaneType {
        fn color(&self) -> Color {
            match *self {
                PaneType::Information => INFORMATION,
                PaneType::History => HISTORY,
                PaneType::Settings => SETTINGS,
                PaneType::Tile => TILE,
            }
        }
    }
    
    pub struct TitleBar {
        pub pane_type: PaneType
    }
    impl container::StyleSheet for TitleBar {
        fn style(&self) -> container::Style {
            container::Style {
                background: Some(Pane{ pane_type: self.pane_type }.style().border_color.into()),
                border_width: 2.0,
                border_radius: 3.0,
                text_color: Some(TITLE_TEXT_COLOR),
                ..Default::default()
            }
        }
    }

    pub struct Pane {
        pub pane_type: PaneType
    }
    impl container::StyleSheet for Pane {
        fn style(&self) -> container::Style {
            container::Style {
                background: Some(Background::Color(Color::WHITE)),
                border_width: 2.0,
                border_radius: 4.0,
                border_color: self.pane_type.color(),
                ..Default::default()
            }
        }
    }

    pub struct ColorButton {
        pub color: Color
    }
    impl iced::button::StyleSheet for ColorButton {
        fn active(&self) -> iced::button::Style {
            iced::button::Style {
                border_radius: 12.0,
                background: Some(Background::Color(self.color)),
                shadow_offset: iced::Vector::new(1.0, 1.0),
                ..Default::default()
            }
        }

        fn hovered(&self) -> iced::button::Style {
            iced::button::Style {
                shadow_offset: iced::Vector::new(1.0, 2.0),
                ..self.active()
            }
        }
    }
}

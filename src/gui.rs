
use iced::{
    pane_grid, scrollable,
    Button, Column, Command, Container, Element,
    HorizontalAlignment, Length,
    PaneGrid, Scrollable, Text, VerticalAlignment,
};
use std::time::{Duration, Instant};

use crate::engine::*;
use crate::data::{
    SmashbrosData, SmashbrosDataTrait,
    SMASHBROS_RESOURCE
};


#[derive(Debug, Clone, Copy)]
pub enum Message {
    None,
    ButtonPressed,
    Tick(Instant),
    TitleClicked(pane_grid::Pane),
    TileClicked(pane_grid::Pane),
}

/* GUIを管理するクラス */
pub struct GUI {
    engine: SmashBrogEngine,
    count: i32,
    pane_battle_infomation: pane_grid::State<ContentBattleInfomation>,
    pane_battle_history: pane_grid::State<ContentBattleHistory>,
    pane_settings: pane_grid::State<ContentSettings>,
}
impl Default for GUI {
    fn default() -> Self {
        unsafe { CAPTION.set(String::from("")).unwrap() };

        Self {
            engine: Default::default(),
            count: 0,
            pane_battle_infomation: pane_grid::State::new(ContentBattleInfomation::new()).0,
            pane_battle_history: pane_grid::State::new(ContentBattleHistory::new()).0,
            pane_settings: pane_grid::State::new(ContentSettings::new()).0,
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
    fn title(&self) -> String { format!("{}|{}", Self::get_title(), self.count) }

    // ここで iced と winit の Event を相互変換したり、独自の Event を発行する
    fn subscription(&self) -> iced::Subscription<Message> {
        iced_winit::subscription::events_with(|event, status| {
            if let iced_winit::event::Status::Captured = status {
                return None;
            }

            match event {
                iced_winit::Event::Mouse(event) => match event {
                    iced::mouse::Event::ButtonPressed(_) => {
                        Some(Message::ButtonPressed)
                    },
                    _ => None,
                },
                _ => None,
            }
        });

        // iced でタイマー処理するには、非同期にイベントを発行しなければいけないらしい
        time::every(Duration::from_millis(1000/15))
            .map(|instant| Message::Tick(instant))
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
            Message::ButtonPressed => {
                self.count = 0;
            },
            Message::Tick(_) => {
                self.count += 1;
                match self.engine.update() {
                    Ok(_) => {
                        // no problem
                    },
                    Err(e) => {
                        // quit
                        println!("quit. [{}]", e);
                        std::process::exit(1);
                    }
                }
            },
            _ => (),
        }

        Command::none()
    }

    // ui の表示(静的でなく動的で書き換えられる)
    fn view(&mut self) -> Element<Message> {
        use std::sync::{ Arc, Mutex };
        let data = Arc::new(Mutex::new( self.engine.get_now_data() ));
        let pane_battle_infomation = PaneGrid::new(&mut self.pane_battle_infomation, |pane, content| {
                let data = data.clone();
                let data: SmashbrosData = (*data.lock().unwrap()).clone();
                let title_bar = pane_grid::TitleBar::new(Text::new("Battle Infomation:"))
                    .padding(10)
                    .style(style::TitleBar{ pane_type: style::PaneType::Information });

                pane_grid::Content::new( content.view(pane, data) )
                    .title_bar(title_bar)
                    .style(style::Pane{ pane_type: style::PaneType::Information })
            })
            .width(Length::Fill)
            .height(Length::FillPortion(2))
            .on_click(Message::TitleClicked);

        let data_list = Arc::new(Mutex::new( self.engine.get_data_latest_10() ));
        let pane_battle_history = PaneGrid::new(&mut self.pane_battle_history, |pane, content| {
                let data_list = data_list.clone();
                let data_list: Vec<SmashbrosData> = (*data_list.lock().unwrap()).clone();
                let title_bar = pane_grid::TitleBar::new(Text::new("Battle History:"))
                    .padding(10)
                    .style(style::TitleBar{ pane_type: style::PaneType::History });

                pane_grid::Content::new( content.view(pane, data_list) )
                    .title_bar(title_bar)
                    .style(style::Pane{ pane_type: style::PaneType::History })
            })
            .width(Length::Fill)
            .height(Length::FillPortion(6))
            .on_click(Message::TitleClicked);

        let pane_settings = PaneGrid::new(&mut self.pane_settings, |pane, content| {
                let title_bar = pane_grid::TitleBar::new(Text::new("Settings:"))
                    .padding(10)
                    .style(style::TitleBar{ pane_type: style::PaneType::Settings });

                pane_grid::Content::new( content.view(pane) )
                    .title_bar(title_bar)
                    .style(style::Pane{ pane_type: style::PaneType::Settings })
            })
            .width(Length::Fill)
            .height(Length::FillPortion(2))
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

    fn view(&mut self, pane: pane_grid::Pane, data: SmashbrosData) -> Element<Message> {
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

    fn view(&mut self, pane: pane_grid::Pane, data_list: Vec<SmashbrosData>) -> Element<Message> {
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
    apply_button: iced::button::State,
    prev_time: std::time::Instant,
    dummy_button: iced::button::State,
}
impl ContentSettings {
    fn new() -> Self {
        Self {
            apply_button: iced::button::State::new(),
            prev_time: std::time::Instant::now(),
            dummy_button: iced::button::State::new(),
        }
    }

    fn view(&mut self, pane: pane_grid::Pane) -> Element<Message> {
        let mut controlls = Column::new()
            .spacing(5);

        let mut fps_row = iced::Row::new()
            .align_items(iced::Align::Center)
            .push(Text::new("Job: "));
        let now = std::time::Instant::now();
        fps_row = match self.prev_time.elapsed().as_millis() as i32 {
              0 ..=  99 => fps_row.push(
                Button::new(&mut self.dummy_button, Text::new("Good"))
                    .style(style::ColorButton{ color: style::SUCCESS_COLOR })
                    .on_press(Message::ButtonPressed),
            ),
            100 ..= 999 => fps_row.push(
                Button::new(&mut self.dummy_button, Text::new("Uhh."))
                    .style(style::ColorButton{ color: style::INFO_COLOR })
                    .on_press(Message::ButtonPressed),
            ),
            _ => fps_row.push(
                Button::new(&mut self.dummy_button, Text::new("Busy"))
                    .style(style::ColorButton{ color: style::WARNING_COLOR })
                    .on_press(Message::ButtonPressed),
            ),
        };
        self.prev_time = now;
        controlls = controlls.push(fps_row);
        
        controlls = controlls.push(
            Button::new(&mut self.apply_button,
                    Text::new("Apply").horizontal_alignment(HorizontalAlignment::Center)
            )
            .width(Length::Fill)
            .on_press(Message::ButtonPressed),
        );

        Container::new(controlls)
            .padding(5)
            .into()
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
    fn push_chara<'a>(&mut self, row: iced::Row<'a, Message>, chara_name: String, text: &str) -> iced::Row<'a, Message> {
        if let Some(handle) = unsafe{SMASHBROS_RESOURCE.get()}.get_image_handle(chara_name.clone()) {
            row.push(
                iced::image::Image::new(handle)
            )
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

    fn view(&mut self, pane: pane_grid::Pane, data: SmashbrosData) -> Element<Message> {
        let mut row = iced::Row::new()
            .spacing(5)
            .align_items(iced::Align::Center);

        if data.get_player_count() < 2 {
            row = row.push(
                Text::new("unknown data.")
                    .width(Length::Fill)
                    .height(Length::from(32))
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .vertical_alignment(VerticalAlignment::Center)
            );
            return Container::new(row)
                .style(style::Pane{ pane_type: style::PaneType::Tile })
                .into();
        }
        
        row = self.push_chara(row, data.get_character(0).clone(), "1p");
        row = row.push(Text::new("vs"));
        row = self.push_chara(row, data.get_character(1).clone(), "2p");

        row = row.push(
            iced::Column::new()
                .align_items(iced::Align::Start)
                .push(
                    Text::new(format!("Rule: {:?}",
                            data.get_rule()
                        ))
                        .width(Length::Fill)
                        .size(14)
                )
                .push(
                    Text::new(format!("Stock: {} - {} / {} - {}",
                            data.get_stock(0), data.get_stock(1), data.get_max_stock(0), data.get_max_stock(1)
                        ))
                        .width(Length::Fill)
                        .size(14)
                )
        );

        Container::new(row)
            .style(style::Pane{ pane_type: style::PaneType::Tile })
            .into()
    }
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

// 非同期に Subscription へ Instant を duration 毎に予約するモジュール
mod time {
    use iced::futures;
    use std::time::Instant;

    pub fn every(duration: std::time::Duration) -> iced::Subscription<Instant> {
        iced::Subscription::from_recipe(Every(duration))
    }

    struct Every(std::time::Duration);
    impl<H, I> iced_native::subscription::Recipe<H, I> for Every
        where H: std::hash::Hasher,
    {
        type Output = Instant;

        fn hash(&self, state: &mut H) {
            use std::hash::Hash;

            std::any::TypeId::of::<Self>().hash(state);
            self.0.hash(state);
        }

        // Recipe から Instant への変換
        fn stream(
            self: Box<Self>,
            _input: futures::stream::BoxStream<'static, I>,
        ) -> futures::stream::BoxStream<'static, Self::Output> {
            use futures::stream::StreamExt;

            tokio::time::interval(self.0)
                .map(|_| Instant::now())
                .boxed()
        }
    }
}

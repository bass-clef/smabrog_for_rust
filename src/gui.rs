
use iced_winit::{
    Button, Column, Command, Element, Text,
};
use std::time::{Duration, Instant};

use crate::engine::*;

/* GUIを管理するクラス */
#[derive(Default)]
pub struct GUI {
    button: iced_winit::button::State,
    count: i32,
    engine: SmashBrogEngine,
}
#[derive(Debug, Clone, Copy)]
pub enum Message {
    Tick(Instant),
    None,
    ButtonPressed,
}
// 実態(作成前?)
impl iced_winit::application::Application for GUI {
    type Flags = ();

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (
            Default::default(),
            Command::none()
        )
    }
    fn title(&self) -> String { format!("{}. [{}]", GUI::get_title(), self.count) }

    // ここで iced と winit の Event を相互変換したり、独自の Event を発行する
    fn subscription(&self) -> iced_winit::subscription::Subscription<Message> {
        // どうやら iced_winit のイベントを独自イベントに書き換えてるらしい
        let event = iced_winit::subscription::events_with(|event, status| {
            if let iced_winit::event::Status::Captured = status {
                return None;
            }

            match event {
                iced_winit::Event::Mouse(event) => match event {
                    iced_winit::mouse::Event::ButtonPressed(_) => {
                        Some(Message::ButtonPressed)
                    },
                    _ => None,
                },
                _ => None,
            }
        });

        // iced でタイマー処理するには async-std で非同期にイベントを発行しなければいけないらしい
        time::every(Duration::from_millis(1000 / 60))
            .map(|instant| Message::Tick(instant))
    }
}

// 外観
impl iced_winit::Program for GUI {
    type Renderer = iced_wgpu::Renderer;
    type Message = Message;

    // subscription で予約発行されたイベントの処理
    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::ButtonPressed => {
                self.count = 0;
            },
            Message::Tick(_) => {
                self.count += 1;
                match self.engine.update() {
                    Ok(_) => {
                        // no progrem
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
    fn view(&mut self) -> Element<Message, iced_wgpu::Renderer> {
        Column::new()
            .push(
                iced_winit::widget::container::Container::new(
                    Text::new("")
                )
            )
            .push(
                Button::new(&mut self.button, Text::new("test button"))
                    .width(iced_winit::Length::Fill)
                    .on_press(Message::ButtonPressed),
            )
            .into()
    }
}
use once_cell::sync::OnceCell;
static INSTANCE: OnceCell<String> = OnceCell::new();
impl GUI {
    pub fn get_title() -> &'static str {
        match INSTANCE.get() {
            Some(v) => &v,
            None => "",
        }
    }
    pub fn set_title(new_title: &str) {
        match INSTANCE.set(new_title.to_string()) {
            Ok(_) => (),
            _ => (),
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

            async_std::stream::interval(self.0)
                .map(|_| Instant::now())
                .boxed()
        }
    }
}

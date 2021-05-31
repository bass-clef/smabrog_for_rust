/*

    smabrog

    author:Humi@bass_clef_
    e-mail:bassclef.nico@gmail.com

*/

use smabrog::gui::*;

/* メインループ */
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    make_gui_run().unwrap();
    Ok(())
}

fn make_gui_run() -> Result<(), iced_winit::Error> {
    // ウィンドウの作成,GUI変数の定義,レンダリングの設定と iced への処理の移譲
    let window = iced_winit::settings::Window {
        size: (256, 720),
        min_size: Some((256, 256)), max_size: Some((256, 720)),
        ..iced_winit::settings::Window::default()
    };
    let settings = iced_winit::Settings::<()> {
        window: window,
        flags: (),
        exit_on_close_request: true
    };
    let renderer_settings = iced_wgpu::Settings {
        antialiasing: Some(iced_wgpu::settings::Antialiasing::MSAAx4),
        default_text_size: 18,
        default_font: Some(include_bytes!("../fonts/NotoSans-Regular.ttf")),
        ..iced_wgpu::Settings::default()
    };

    iced_winit::application::run::<GUI, iced::executor::Default, iced_wgpu::window::Compositor>(
        settings.into(),
        renderer_settings,
    )
}

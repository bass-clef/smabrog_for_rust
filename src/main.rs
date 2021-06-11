/*

    smabrog

    author:Humi@bass_clef_
    e-mail:bassclef.nico@gmail.com

*/

use smabrog::gui::GUI;

/* メインループ */
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();
    make_gui_run().unwrap();

    Ok(())
}

fn init_logger(){
    let base_config = fern::Dispatch::new();
 
    let file_config = fern::Dispatch::new()
        .level(log::LevelFilter::Error)
        .level_for("smabrog", log::LevelFilter::Debug)
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.level(),
                record.target(),
                message
            ))
        })
        .chain(fern::log_file("latest.log").unwrap());
 
    let stdout_config = fern::Dispatch::new()
        .level(log::LevelFilter::Error)
        .level_for("smabrog", log::LevelFilter::Debug)
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%H:%M:%S]"),
                record.level(),
                record.target(),
                message
            ))
        })
        .chain(std::io::stdout());
 
    base_config
        .chain(file_config)
        .chain(stdout_config)
        .apply().unwrap();
}

fn make_gui_run() -> Result<(), iced_winit::Error> {
    use opencv::prelude::MatTrait;
    // ウィンドウの作成,GUI変数の定義,レンダリングの設定と iced への処理の移譲
    let window_icon = opencv::imgcodecs::imread("icon/smabrog.png", opencv::imgcodecs::IMREAD_UNCHANGED).unwrap();
    let icon_size = ( window_icon.cols() * window_icon.rows() * 4 ) as usize;
    let icon_data_by_slice: &[u8] = unsafe{ std::slice::from_raw_parts(window_icon.datastart(), icon_size) };

    let window = iced_winit::settings::Window {
        size: (256, 720),
        min_size: Some((256, 256)), max_size: Some((256, 720)),
        icon: Some(winit::window::Icon::from_rgba(icon_data_by_slice.to_vec(), window_icon.cols() as u32, window_icon.rows() as u32).unwrap()),
        ..iced_winit::settings::Window::default()
    };
    let settings = iced_winit::Settings::<()> {
        window: window,
        flags: (),
        exit_on_close_request: false
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

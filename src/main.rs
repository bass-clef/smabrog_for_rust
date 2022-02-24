/*

    smabrog

    author:Humi@bass_clef_
    e-mail:bassclef.nico@gmail.com

*/

/* メインループ */
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();

    // smabrog::gui::make_gui_run().unwrap();
    smabrog::egui::run_gui().await.unwrap();

    Ok(())
}

fn init_logger(){
    log_panics::init();

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
        .level_for("smabrog", log::LevelFilter::Info)
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

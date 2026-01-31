mod graphics;
mod app;

use app::MyApp;
use eframe::egui;

fn main() -> eframe::Result {
    env_logger::init();
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([820.0, 680.0])
            .with_title("Print Organizer"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    
    eframe::run_native(
        "Print Organizer",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)))),
    )
}

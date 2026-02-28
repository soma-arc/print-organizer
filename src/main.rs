mod app;
mod graphics;
mod preview_compose;

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
        Box::new(|cc| {
            setup_japanese_fonts(&cc.egui_ctx);
            Ok(Box::new(MyApp::new(cc)))
        }),
    )
}

/// システムの日本語フォントを egui に登録する。
fn setup_japanese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Windows: Yu Gothic UI → Meiryo → MS Gothic の順で試行
    let candidates = [
        (r"C:\Windows\Fonts\YuGothM.ttc", "Yu Gothic UI"),
        (r"C:\Windows\Fonts\meiryo.ttc", "Meiryo"),
        (r"C:\Windows\Fonts\msgothic.ttc", "MS Gothic"),
    ];

    for (path, name) in &candidates {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                name.to_string(),
                egui::FontData::from_owned(font_data).into(),
            );
            // Proportional（UI テキスト）と Monospace 両方に追加
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push(name.to_string());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push(name.to_string());
            log::info!("Loaded Japanese font: {name} from {path}");
            break; // 最初に見つかったフォントだけ使う
        }
    }

    ctx.set_fonts(fonts);
}

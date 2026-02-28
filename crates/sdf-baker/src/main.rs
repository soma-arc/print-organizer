use clap::Parser;
use sdf_baker::cli::Cli;

fn main() {
    let cli = Cli::parse();

    // Initialize logger
    env_logger::Builder::new()
        .filter_level(match cli.log_level.as_str() {
            "error" => log::LevelFilter::Error,
            "warn" => log::LevelFilter::Warn,
            "debug" => log::LevelFilter::Debug,
            _ => log::LevelFilter::Info,
        })
        .init();

    log::info!("sdf-baker v{}", env!("CARGO_PKG_VERSION"));
    log::debug!("{cli:#?}");

    // TODO: R1+ pipeline implementation
    log::info!("output dir: {}", cli.out.display());
}

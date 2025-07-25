use anyhow::Result;

use babypi::config::{CliArgs, TomlConfig, TOML_CONFIG_DEFAULT_DIR, TOML_CONFIG_DEFAULT_FILENAME};

use babypi::BabyPi;
use clap::Parser;

use tracing::info;
use tracing_subscriber::{util::SubscriberInitExt, FmtSubscriber};

#[tokio::main]
async fn main() -> Result<()> {
    // cli args
    let args = CliArgs::parse();

    // logging
    let level = if args.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    FmtSubscriber::builder()
        .with_max_level(level)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .finish()
        .init();

    // config
    let config_file = args.config.unwrap_or(format!(
        "{}/{}",
        TOML_CONFIG_DEFAULT_DIR, TOML_CONFIG_DEFAULT_FILENAME
    ));
    let config = TomlConfig::load(&config_file).await?;
    config.validate().await?;

    // init app
    let mut app = BabyPi::new(config);
    // let app_run = app.run();
    // tokio::pin!(app_run);
    app.run().await?;

    // run
    // tokio::select! {
    //     _ = app_run => {
    //         info!(target = "babypi-server", "Application exit");
    //     }
    //     _ = tokio::signal::ctrl_c() => {
    //         info!(target = "babypi-server", "Shutdown signal received");
    //     }
    // }

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl+c");

    info!(target = "babypi-server", "Shutdown signal received");

    app.stop().await?;

    Ok(())
}

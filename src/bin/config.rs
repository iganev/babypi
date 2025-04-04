use std::str::FromStr;

use anyhow::{anyhow, Result};
use babypi::{
    config::{CliArgs, TomlConfig, TOML_CONFIG_DEFAULT_FILENAME},
    file_exists,
};
use clap::Parser;
use tokio::{fs::OpenOptions, io::AsyncWriteExt};
use tracing::info;
use tracing_subscriber::{util::SubscriberInitExt, FmtSubscriber};

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();

    // Logging
    FmtSubscriber::builder()
        .with_max_level(tracing::Level::from_str("DEBUG")?)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .finish()
        .init();

    if let Some(config) = args.config.as_deref() {
        info!("Loading {}", config);
        let config = tokio::fs::canonicalize(config).await?;

        if file_exists(&config).await {
            let config = TomlConfig::load(&config).await?;

            info!("Config: {:?}", config);
        } else {
            return Err(anyhow!(
                "File does not exist: {}",
                config.to_str().unwrap_or_default()
            ));
        }
    } else {
        info!("Creating default config");

        let config = TomlConfig::new();

        let config_content = toml::to_string_pretty(&config)?;

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o644)
            .open(TOML_CONFIG_DEFAULT_FILENAME)
            .await?;

        file.write_all(config_content.as_bytes()).await?;
        file.flush().await?;
    }

    Ok(())
}

use std::str::FromStr;

use babypi::rpicam::Rpicam;
use tracing::info;
use tracing_subscriber::{util::SubscriberInitExt, FmtSubscriber};

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Logging
    FmtSubscriber::builder()
        .with_max_level(tracing::Level::from_str("DEBUG")?)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .finish()
        .init();

    let res = Rpicam::list_cameras().await?;

    info!("Devices: {:?}", res);

    Ok(())
}

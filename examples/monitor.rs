use std::{str::FromStr, time::Duration};

use babypi::{
    audio_monitor::{AudioMonitor, AudioMonitorContext},
    ffmpeg::audio::FfmpegAudioSampleFormat,
    telemetry::events::Event,
};
use tokio::sync::broadcast::channel;
use tracing::info;
use tracing_subscriber::{util::SubscriberInitExt, FmtSubscriber};

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Logging
    FmtSubscriber::builder()
        .with_max_level(tracing::Level::from_str("INFO")?)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .finish()
        .init();

    let (tx, mut rx) = channel::<Event>(10);

    let mut monitor = AudioMonitor::new(
        AudioMonitorContext::new(
            FfmpegAudioSampleFormat::S16le.into(),
            44_100,
            1,
            Some("alsa_input.usb-DCMT_Technology_USB_Lavalier_Microphone_214b206000000178-00.mono-fallback".to_string()),
            Some(0.01),
        ),
        Some(tx),
    );

    let _ = monitor.start().await;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                info!("State: {}", monitor.is_running());
            }
            event = rx.recv() => {
                if let Ok(Event::AudioMonitor { rms }) = event {
                    info!("RMS: {:.3}", rms);
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received");
                monitor.stop();
                break;
            }
        }
    }

    info!("Bye");

    Ok(())
}

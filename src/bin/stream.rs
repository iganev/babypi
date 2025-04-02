use std::{path::PathBuf, str::FromStr};

use babypi::{
    ffmpeg::{
        Ffmpeg, FfmpegAudio, FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE,
        FFMPEG_DEFAULT_AUDIO_SAMPLE_FORMAT, FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE,
    },
    live_stream::LiveStream,
    rpicam::{Rpicam, RpicamCodec},
};
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

    let cam = Rpicam::new(
        None,
        Some(RpicamCodec::default()),
        None,
        PathBuf::from_str("/usr/share/libcamera/ipa/rpi/vc4/imx219_noir.json").ok(),
        None,
    );

    let ffmpeg_audio = FfmpegAudio::new(
        babypi::ffmpeg::FfmpegAudioDeviceType::Pulse,
        "alsa_input.usb-DCMT_Technology_USB_Lavalier_Microphone_214b206000000178-00.mono-fallback", //"hw:3,0",
        Some(FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE),
        Some(FFMPEG_DEFAULT_AUDIO_SAMPLE_FORMAT.to_string()),
        Some(1),
        Some(babypi::ffmpeg::FfmpegAudioFormat::Aac),
        Some(FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE.to_string()),
    );

    let ffmpeg = Ffmpeg::new("/var/stream", Some(ffmpeg_audio), None);

    let mut live_stream = LiveStream::new(cam, ffmpeg);

    live_stream.start().await?;

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl+c");

    info!("Shutdown signal received");

    live_stream.stop().await;

    info!("Bye");

    Ok(())
}

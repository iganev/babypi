use std::{path::PathBuf, str::FromStr, time::Duration};

use babypi::{
    ffmpeg::{
        audio::FfmpegAudio, audio::FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE,
        audio::FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE, Ffmpeg,
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
        true,
        true,
        None,
    );

    let ffmpeg_audio = FfmpegAudio::new(
        babypi::ffmpeg::audio::FfmpegAudioDeviceType::Pulse,
        "alsa_input.usb-DCMT_Technology_USB_Lavalier_Microphone_214b206000000178-00.mono-fallback", //"hw:3,0",
        Some(FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE),
        Some(babypi::ffmpeg::audio::FfmpegAudioSampleFormat::S16le),
        Some(1),
        Some(babypi::ffmpeg::audio::FfmpegAudioFormat::Aac),
        Some(FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE.to_string()),
    );

    let ffmpeg = Ffmpeg::new("/var/stream", Some(ffmpeg_audio), None);

    let live_stream = LiveStream::new(cam, ffmpeg);

    live_stream.start().await;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                info!("State: {}", live_stream.is_running().await);
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received");
                live_stream.stop().await;
                break;
            }
        }
    }

    info!("Bye");

    Ok(())
}

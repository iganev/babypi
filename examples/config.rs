use std::str::FromStr;

use anyhow::{anyhow, Result};
use babypi::{
    config::{
        AccelerometerConfigV1, CameraConfigV1, CliArgs, IrCamConfigV1, MicrophoneConfigV1,
        MmWaveConfigV1, TomlConfig, TomlConfigHardwareV1, TomlConfigMonitoringV1,
        TomlConfigNotificationsV1, TomlConfigRecordingV1, TomlConfigServerV1, TomlConfigStreamV1,
        TomlConfigTelemetryV1, TomlParity, TOML_CONFIG_DEFAULT_FILENAME,
    },
    ffmpeg::{
        audio::{
            FfmpegAudioDeviceType, FfmpegAudioFormat, FfmpegAudioSampleFormat,
            FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE, FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE,
        },
        FFMPEG_DEFAULT_STREAM_DIR,
    },
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

        let config = TomlConfig {
            hardware: TomlConfigHardwareV1 {
                camera: CameraConfigV1 {
                    device_index: Some(0),
                    device: None,
                    codec: Some(babypi::rpicam::RpicamCodec::H264),
                    width: Some(1920),
                    height: Some(1080),
                    fps: Some(30),
                    tuning_file: Some("/usr/share/libcamera/ipa/rpi/vc4/imx219_noir.json".into()),
                    hflip: Some(true),
                    vflip: Some(true),
                    extra_args: Some("".to_string()),
                    ircut_gpio_pin: Some(23),
                    ircut_on_state: Some(true),
                },
                ircam: IrCamConfigV1 {
                    enabled: true,
                    scale: Some(20),
                    offset_x: Some(100),
                    offset_y: Some(100),
                    hflip: Some(true),
                    vflip: Some(true),
                },
                mmwave: MmWaveConfigV1 {
                    enabled: true,
                    gpio_pin: Some(18),
                    baud_rate: Some(115_200),
                    parity: Some(TomlParity::None),
                    data_bits: Some(8),
                    stop_bits: Some(1),
                },
                mic: MicrophoneConfigV1 {
                    enabled: true,
                    interface: Some(FfmpegAudioDeviceType::Pulse),
                    device: Some("alsa_input.usb-DCMT_Technology_USB_Lavalier_Microphone_214b206000000178-00.mono-fallback".to_string()),
                    sample_rate: Some(FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE),
                    sample_format: Some(FfmpegAudioSampleFormat::S16le),
                    channels: Some(1),
                    output_format: Some(FfmpegAudioFormat::Aac),
                    output_bitrate: Some(FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE.to_string()),
                },
                accelerometer: AccelerometerConfigV1 {
                    enabled: true,
                    device: Some("/dev/ttyACM0".to_string()),
                    baud_rate: Some(115_200),
                    parity: Some(TomlParity::None),
                    data_bits: Some(8),
                    stop_bits: Some(1),
                },
            },
            stream: TomlConfigStreamV1 {
                audio: Some(true),
                data_dir: Some(FFMPEG_DEFAULT_STREAM_DIR.into()),
                extra_args_setup: Some("".to_string()),
                extra_args_video_input: Some("".to_string()),
                extra_args_audio_input: Some("".to_string()),
                extra_args_output: Some("".to_string()),
            },
            server: TomlConfigServerV1 {
                bind: Some("0.0.0.0:8080".to_string()),
                bearer_token: Some("bearer_token".to_string()),
                basic_username:Some("admin".to_string()),
                basic_password: Some("password".to_string()), 
                webroot: Some("/var/lib/babypi/static".to_string()) 
            },
            recording: TomlConfigRecordingV1 { enabled: true },
            monitoring: TomlConfigMonitoringV1 {
                enabled: true,
                rms_threshold: Some(0.1),
            },
            telemetry: TomlConfigTelemetryV1 { enabled: true },
            notifications: TomlConfigNotificationsV1 {
                browser: Some("push".to_string()),
                pushover: Some("push".to_string()),
                homeassistant: Some("hass".to_string()),
                mqtt: Some("mqtt".to_string()),
            },
        };

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

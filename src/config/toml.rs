use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use rppal::uart::Parity;
use serde::{Deserialize, Serialize};

use crate::{
    ffmpeg::{
        audio::{FfmpegAudioDeviceType, FfmpegAudioFormat, FfmpegAudioSampleFormat},
        FFMPEG_DEFAULT_STREAM_DIR,
    },
    file_exists,
    rpicam::{Rpicam, RpicamCodec, RpicamDevice, RpicamDeviceMode},
};

pub const TOML_CONFIG_DEFAULT_DIR: &str = "/etc/babypi";
pub const TOML_CONFIG_DEFAULT_FILENAME: &str = "Config.toml";

pub type TomlConfig = TomlConfigV1;

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct TomlConfigV1 {
    pub hardware: TomlConfigHardwareV1,
    pub stream: TomlConfigStreamV1,
    pub server: TomlConfigServerV1,
    pub recording: TomlConfigRecordingV1,
    pub monitoring: TomlConfigMonitoringV1,
    pub telemetry: TomlConfigTelemetryV1,
    pub notifications: TomlConfigNotificationsV1,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct TomlConfigHardwareV1 {
    pub camera: CameraConfigV1,
    pub ircam: IrCamConfigV1,
    pub mmwave: MmWaveConfigV1,
    pub mic: MicrophoneConfigV1,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct TomlConfigStreamV1 {
    pub auth: Option<bool>,
    pub audio: Option<bool>,
    pub data_dir: Option<PathBuf>,
    pub extra_args_setup: Option<String>,
    pub extra_args_video_input: Option<String>,
    pub extra_args_audio_input: Option<String>,
    pub extra_args_output: Option<String>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct TomlConfigServerV1 {
    pub bind: Option<String>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct TomlConfigRecordingV1 {
    pub enabled: bool,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct TomlConfigMonitoringV1 {
    pub enabled: bool,
    pub rms_threshold: Option<u32>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct TomlConfigTelemetryV1 {
    pub enabled: bool,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct TomlConfigNotificationsV1 {
    pub browser: Option<bool>,
    pub pushover: Option<String>,
    pub homeassistant: Option<String>,
    pub mqtt: Option<String>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct CameraConfigV1 {
    pub device_index: Option<u32>,
    #[serde(skip)]
    pub device: Option<RpicamDevice>,
    pub codec: Option<RpicamCodec>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<u32>,
    pub tuning_file: Option<PathBuf>,
    pub hflip: Option<bool>,
    pub vflip: Option<bool>,
    pub extra_args: Option<String>,
    pub ircut_gpio_pin: Option<u8>,
    pub ircut_on_state: Option<bool>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct MicrophoneConfigV1 {
    pub enabled: bool,
    pub interface: Option<FfmpegAudioDeviceType>,
    pub device: Option<String>,
    pub sample_rate: Option<u32>,
    pub sample_format: Option<FfmpegAudioSampleFormat>,
    pub channels: Option<u8>,
    pub output_format: Option<FfmpegAudioFormat>,
    pub output_bitrate: Option<String>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct IrCamConfigV1 {
    pub enabled: bool,
    pub scale: Option<u32>,
    pub offset_x: Option<u32>,
    pub offset_y: Option<u32>,
    pub hflip: Option<bool>,
    pub vflip: Option<bool>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct MmWaveConfigV1 {
    pub enabled: bool,
    pub gpio_pin: Option<u32>,
    pub baud_rate: Option<u32>,
    pub parity: Option<TomlParity>,
    pub data_bits: Option<u8>,
    pub stop_bits: Option<u8>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub enum TomlParity {
    #[default]
    None,
    Even,
    Odd,
    Mark,
    Space,
}

impl From<TomlParity> for Parity {
    fn from(value: TomlParity) -> Self {
        match value {
            TomlParity::None => Parity::None,
            TomlParity::Even => Parity::Even,
            TomlParity::Odd => Parity::Odd,
            TomlParity::Mark => Parity::Mark,
            TomlParity::Space => Parity::Space,
        }
    }
}

impl TomlConfigV1 {
    /// Load config from .toml file and initialize
    pub async fn load(file: impl AsRef<Path>) -> Result<Self> {
        match tokio::fs::read_to_string(file).await {
            Ok(c) => {
                let mut config: TomlConfigV1 = toml::from_str(&c)
                    .map_err(|e| anyhow!("Failed to parse toml config: {}", e))?;

                let cameras = Rpicam::list_cameras().await?;
                let device_index = config.hardware.camera.device_index.unwrap_or(0);
                config.hardware.camera.device = cameras.get(device_index as usize).cloned();

                if config.hardware.camera.codec.is_none() {
                    config.hardware.camera.codec = Some(RpicamCodec::default());
                }

                Ok(config)
            }
            Err(e) => Err(anyhow!("Failed to load profile config: {}", e).into()),
        }
    }

    /// Create new default config
    pub fn new() -> Self {
        TomlConfigV1 {
            hardware: TomlConfigHardwareV1::default(),
            stream: TomlConfigStreamV1::default(),
            server: TomlConfigServerV1::default(),
            recording: TomlConfigRecordingV1::default(),
            monitoring: TomlConfigMonitoringV1::default(),
            telemetry: TomlConfigTelemetryV1::default(),
            notifications: TomlConfigNotificationsV1::default(),
        }
    }

    /// Check declared values validity
    pub async fn validate(&self) -> Result<()> {
        let camera_index = self.hardware.camera.device_index.unwrap_or(0) as usize;
        let camera_mode = if let Some(w) = self.hardware.camera.width {
            if let Some(h) = self.hardware.camera.height {
                if let Some(fps) = self.hardware.camera.fps {
                    RpicamDeviceMode::new("selected", w, h, fps)
                } else {
                    RpicamDeviceMode::default()
                }
            } else {
                RpicamDeviceMode::default()
            }
        } else {
            RpicamDeviceMode::default()
        };

        // let cameras = Rpicam::list_cameras().await?;

        if let Some(camera) = self.hardware.camera.device.as_ref() {
            if !camera.modes.iter().any(|mode| {
                mode.fps >= camera_mode.fps
                    && mode.width == camera_mode.width
                    && mode.height == camera_mode.height
            }) {
                return Err(anyhow!(
                    "Camera `{}` does not support selected mode {}x{} at {} fps.",
                    camera_index,
                    camera_mode.width,
                    camera_mode.height,
                    camera_mode.fps
                ));
            }
        } else {
            return Err(anyhow!("Camera `{}` not found.", camera_index));
        }

        if let Some(tuning_file) = self.hardware.camera.tuning_file.as_ref() {
            if !file_exists(tuning_file).await {
                return Err(anyhow!("Camera tuning file is invalid."));
            }
        }

        let data_dir = self
            .stream
            .data_dir
            .clone()
            .unwrap_or(FFMPEG_DEFAULT_STREAM_DIR.into());

        if !file_exists(&data_dir).await {
            return Err(anyhow!("Stream storage directory is invalid."));
        }

        if self.monitoring.enabled {
            if !self.hardware.mic.enabled {
                return Err(anyhow!(
                    "Audio monitoring can't be enabled without enabled microphone config."
                ));
            } else if self
                .hardware
                .mic
                .interface
                .as_ref()
                .is_none_or(|interface| interface == &FfmpegAudioDeviceType::Alsa)
            {
                return Err(anyhow!(
                    "Audio monitoring can't be enabled when using ALSA."
                ));
            }
        }

        Ok(())
    }
}

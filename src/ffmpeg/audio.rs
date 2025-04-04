use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

pub static FFMPEG_DEFAULT_AUDIO_DEVICE: &str = "hw:1,0";
pub static FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE: u32 = 48_000;
pub static FFMPEG_DEFAULT_AUDIO_SAMPLE_FORMAT: &str = "s16le";
pub static FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE: &str = "128k";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum FfmpegAudioFormat {
    #[default]
    Aac,
    Mp3,
}

impl Display for FfmpegAudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FfmpegAudioFormat::Aac => write!(f, "aac"),
            FfmpegAudioFormat::Mp3 => write!(f, "libmp3lame"),
        }
    }
}

impl FromStr for FfmpegAudioFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "aac" => Ok(FfmpegAudioFormat::Aac),
            "libmp3lame" => Ok(FfmpegAudioFormat::Mp3),
            _ => Err(anyhow!("Invalid audio format")),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum FfmpegAudioDeviceType {
    #[default]
    Alsa,
    Pulse,
}

impl Display for FfmpegAudioDeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FfmpegAudioDeviceType::Alsa => write!(f, "alsa"),
            FfmpegAudioDeviceType::Pulse => write!(f, "pulse"),
        }
    }
}

impl FromStr for FfmpegAudioDeviceType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "alsa" => Ok(FfmpegAudioDeviceType::Alsa),
            "pulse" => Ok(FfmpegAudioDeviceType::Pulse),
            _ => Err(anyhow!("Invalid audio device type")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FfmpegAudio {
    pub device_type: FfmpegAudioDeviceType,
    pub device_node: String,
    pub sample_rate: Option<u32>,
    pub sample_format: Option<String>,
    pub channels: Option<u8>,
    pub output_format: Option<FfmpegAudioFormat>,
    pub output_bitrate: Option<String>,
}

impl Default for FfmpegAudio {
    fn default() -> Self {
        Self {
            device_type: FfmpegAudioDeviceType::Alsa,
            device_node: FFMPEG_DEFAULT_AUDIO_DEVICE.to_string(),
            sample_rate: Some(FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE),
            sample_format: Some(FFMPEG_DEFAULT_AUDIO_SAMPLE_FORMAT.to_string()),
            channels: Some(1),
            output_format: Default::default(),
            output_bitrate: Some(FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE.to_string()),
        }
    }
}

impl FfmpegAudio {
    pub fn new(
        device_type: FfmpegAudioDeviceType,
        device_node: impl ToString,
        sample_rate: Option<u32>,
        sample_format: Option<String>,
        channels: Option<u8>,
        output_format: Option<FfmpegAudioFormat>,
        output_bitrate: Option<String>,
    ) -> Self {
        Self {
            device_type,
            device_node: device_node.to_string(),
            sample_rate,
            sample_format,
            channels,
            output_format,
            output_bitrate,
        }
    }
}

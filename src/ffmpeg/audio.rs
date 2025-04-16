use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

pub static FFMPEG_DEFAULT_AUDIO_DEVICE: &str = "hw:1,0";
pub static FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE: u32 = 44_100;
pub static FFMPEG_DEFAULT_AUDIO_SAMPLE_FORMAT: &str = "s16le";
pub static FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE: &str = "128k";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum FfmpegAudioSampleFormat {
    // /// Unsigned 8 Bit PCM.
    // U8,
    // /// 8 Bit a-Law.
    // ALaw,
    // /// 8 Bit mu-Law.
    // ULaw,
    #[default]
    /// Signed 16 Bit PCM, little endian (PC).
    S16le,
    /// Signed 16 Bit PCM, big endian.
    // S16be,
    /// 32 Bit IEEE floating point, little endian (PC), range -1.0 to 1.0.
    F32le,
    /// 32 Bit IEEE floating point, big endian, range -1.0 to 1.0.
    // F32be,
    /// Signed 32 Bit PCM, little endian (PC).
    S32le,
    // /// Signed 32 Bit PCM, big endian.
    // S32be,
    // /// Signed 24 Bit PCM packed, little endian (PC).
    // S24le,
    // /// Signed 24 Bit PCM packed, big endian.
    // S24be,
    // /// Signed 24 Bit PCM in LSB of 32 Bit words, little endian (PC).
    // S24_32le,
    // /// Signed 24 Bit PCM in LSB of 32 Bit words, big endian.
    // S24_32be,
}

impl Display for FfmpegAudioSampleFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // FfmpegAudioSampleFormat::U8 => write!(f, "u8"),
            // FfmpegAudioSampleFormat::ALaw => write!(f, "alaw"),
            // FfmpegAudioSampleFormat::ULaw => write!(f, "ulaw"),
            FfmpegAudioSampleFormat::S16le => write!(f, "s16le"),
            // FfmpegAudioSampleFormat::S16be => write!(f, "s16be"),
            FfmpegAudioSampleFormat::F32le => write!(f, "f32le"),
            // FfmpegAudioSampleFormat::F32be => write!(f, "f32be"),
            FfmpegAudioSampleFormat::S32le => write!(f, "s32le"),
            // FfmpegAudioSampleFormat::S32be => write!(f, "s32be"),
            // FfmpegAudioSampleFormat::S24le => write!(f, "s24le"),
            // FfmpegAudioSampleFormat::S24be => write!(f, "s24be"),
            // FfmpegAudioSampleFormat::S24_32le => write!(f, "s24_32le"),
            // FfmpegAudioSampleFormat::S24_32be => write!(f, "s24_32le"),
        }
    }
}

impl FromStr for FfmpegAudioSampleFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            // "u8" => Ok(FfmpegAudioSampleFormat::U8),
            // "alaw" => Ok(FfmpegAudioSampleFormat::ALaw),
            // "ulaw" => Ok(FfmpegAudioSampleFormat::ULaw),
            "s16le" => Ok(FfmpegAudioSampleFormat::S16le),
            // "s16be" => Ok(FfmpegAudioSampleFormat::S16be),
            "f32le" => Ok(FfmpegAudioSampleFormat::F32le),
            // "f32be" => Ok(FfmpegAudioSampleFormat::F32be),
            "s32le" => Ok(FfmpegAudioSampleFormat::S32le),
            // "s32be" => Ok(FfmpegAudioSampleFormat::S32be),
            // "s24le" => Ok(FfmpegAudioSampleFormat::S24le),
            // "s24be" => Ok(FfmpegAudioSampleFormat::S24be),
            _ => Err(anyhow!("Invalid audio sample format")),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
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
    pub sample_format: Option<FfmpegAudioSampleFormat>,
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
            sample_format: Some(FfmpegAudioSampleFormat::default()),
            channels: Some(1),
            output_format: Some(FfmpegAudioFormat::default()),
            output_bitrate: Some(FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE.to_string()),
        }
    }
}

impl FfmpegAudio {
    pub fn new(
        device_type: FfmpegAudioDeviceType,
        device_node: impl ToString,
        sample_rate: Option<u32>,
        sample_format: Option<FfmpegAudioSampleFormat>,
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

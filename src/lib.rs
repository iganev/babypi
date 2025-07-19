use std::path::Path;

// use actix_web::dev::ServerHandle;
use anyhow::Result;

// use audio_monitor::AudioMonitor;
use config::TomlConfig;
use ffmpeg::audio::FfmpegAudio;
use ffmpeg::audio::FFMPEG_DEFAULT_AUDIO_DEVICE;
use ffmpeg::Ffmpeg;
use ffmpeg::FfmpegExtraArgs;
use ffmpeg::FFMPEG_DEFAULT_STREAM_DIR;
use live_stream::LiveStream;
use rpicam::Rpicam;
use rpicam::RpicamDeviceMode;

pub mod audio_monitor;
pub mod config;
pub mod ffmpeg;
pub mod gpio;
pub mod live_stream;
pub mod mlx90640;
pub mod mmwave;
pub mod process_control;
pub mod rpicam;
pub mod server;

/// Check if file exists
pub async fn file_exists(file: impl AsRef<Path>) -> bool {
    tokio::fs::try_exists(file).await.is_ok_and(|res| res)
}

#[derive(Debug)]
pub struct BabyPi {
    config: TomlConfig,

    live_stream: Option<LiveStream>,
    // web_server: Option<ServerHandle>,
    // audio_monitor: Option<AudioMonitor>,
}

impl BabyPi {
    pub fn new(config: TomlConfig) -> Self {
        Self {
            config,
            live_stream: None,
            // web_server: None,
            // audio_monitor: None,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        self.live_stream = Some(self.run_live_stream().await?);

        Ok(())
    }

    async fn run_live_stream(&self) -> Result<LiveStream> {
        let mode = if self.config.hardware.camera.width.is_some()
            && self.config.hardware.camera.height.is_some()
            && self.config.hardware.camera.fps.is_some()
        {
            Some(RpicamDeviceMode::new(
                "selected",
                self.config.hardware.camera.width.unwrap(),
                self.config.hardware.camera.height.unwrap(),
                self.config.hardware.camera.height.unwrap(),
            ))
        } else {
            None
        };

        let cam = Rpicam::new(
            self.config.hardware.camera.device.clone(),
            self.config.hardware.camera.codec.clone(),
            mode,
            self.config.hardware.camera.tuning_file.clone(),
            self.config.hardware.camera.hflip.unwrap_or(false),
            self.config.hardware.camera.vflip.unwrap_or(false),
            self.config
                .hardware
                .camera
                .extra_args
                .as_deref()
                .map(|s| s.split(" ").map(str::to_string).collect::<Vec<String>>()),
        );

        let ffmpeg_audio =
            if self.config.stream.audio.is_some_and(|v| v) && self.config.hardware.mic.enabled {
                Some(FfmpegAudio::new(
                    self.config
                        .hardware
                        .mic
                        .interface
                        .clone()
                        .unwrap_or_default(),
                    self.config
                        .hardware
                        .mic
                        .device
                        .as_deref()
                        .unwrap_or(FFMPEG_DEFAULT_AUDIO_DEVICE),
                    self.config.hardware.mic.sample_rate,
                    self.config.hardware.mic.sample_format.clone(),
                    self.config.hardware.mic.channels,
                    self.config.hardware.mic.output_format.clone(),
                    self.config.hardware.mic.output_bitrate.clone(),
                ))
            } else {
                None
            };

        let extra_args = if self.config.stream.extra_args_audio_input.is_some()
            || self.config.stream.extra_args_video_input.is_some()
            || self.config.stream.extra_args_setup.is_some()
            || self.config.stream.extra_args_output.is_some()
        {
            Some(FfmpegExtraArgs {
                setup: self
                    .config
                    .stream
                    .extra_args_setup
                    .as_deref()
                    .map(|s| s.split(" ").map(str::to_string).collect::<Vec<String>>()),
                video_input: self
                    .config
                    .stream
                    .extra_args_video_input
                    .as_deref()
                    .map(|s| s.split(" ").map(str::to_string).collect::<Vec<String>>()),
                audio_input: self
                    .config
                    .stream
                    .extra_args_audio_input
                    .as_deref()
                    .map(|s| s.split(" ").map(str::to_string).collect::<Vec<String>>()),
                output: self
                    .config
                    .stream
                    .extra_args_output
                    .as_deref()
                    .map(|s| s.split(" ").map(str::to_string).collect::<Vec<String>>()),
            })
        } else {
            None
        };

        let ffmpeg = Ffmpeg::new(
            self.config
                .stream
                .data_dir
                .clone()
                .unwrap_or(FFMPEG_DEFAULT_STREAM_DIR.into()),
            ffmpeg_audio,
            extra_args,
        );

        let live_stream = LiveStream::new(cam, ffmpeg);

        live_stream.start().await;

        Ok(live_stream)
    }
}

use std::{path::PathBuf, process::Stdio, str::FromStr};

use audio::FfmpegAudio;
use audio::FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE;
use audio::FFMPEG_DEFAULT_AUDIO_SAMPLE_FORMAT;
use audio::FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE;
use tokio::process::{Child, Command};

use anyhow::anyhow;
use anyhow::Result;

pub static FFMPEG_BIN: &str = "ffmpeg";

pub static FFMPEG_DEFAULT_STREAM_DIR: &str = "/var/run/babypi/stream";
pub static FFMPEG_DEFAULT_STREAM_PLAYLIST_NAME: &str = "live.m3u8";
pub static FFMPEG_DEFAULT_STREAM_SEGMENT_NAME_PATTERN: &str = "%08d.ts";

pub mod audio;

#[derive(Clone, Debug, Default)]
pub struct FfmpegExtraArgs {
    pub setup: Option<Vec<String>>,
    pub video_input: Option<Vec<String>>,
    pub audio_input: Option<Vec<String>>,
    pub output: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct Ffmpeg {
    pub stream_dir: PathBuf,
    pub audio_input: Option<FfmpegAudio>,
    pub extra_args: Option<FfmpegExtraArgs>,
}

impl Default for Ffmpeg {
    fn default() -> Self {
        Self {
            stream_dir: PathBuf::from_str(FFMPEG_DEFAULT_STREAM_DIR)
                .expect("Failed to build path to stream playlist"),
            audio_input: None,
            extra_args: None,
        }
    }
}

impl Ffmpeg {
    pub fn new(
        stream_dir: impl Into<PathBuf>,
        audio_input: Option<FfmpegAudio>,
        extra_args: Option<FfmpegExtraArgs>,
    ) -> Self {
        Self {
            stream_dir: stream_dir.into(),
            audio_input,
            extra_args,
        }
    }

    fn build_ffmpeg_cmd_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        // inject extras
        if let Some(extra_args) = self.extra_args.as_ref() {
            if let Some(setup_args) = extra_args.setup.as_ref() {
                args.extend_from_slice(setup_args);
            }
        }

        // auto-yes
        args.push("-y".to_string());

        // suppress most output
        args.push("-v".to_string());
        args.push("quiet".to_string());
        // args.push("-stats".to_string());

        // read more input before deciding on params
        args.push("-probesize".to_string());
        args.push("32M".to_string());

        // critical for older raspberries that cant keep up
        args.push("-thread_queue_size".to_string());
        args.push("256".to_string());

        // come up with video timestamps
        args.push("-use_wallclock_as_timestamps".to_string());
        args.push("1".to_string());

        // we will be piping the h264 input
        args.push("-i".to_string());
        args.push("pipe:".to_string());

        // inject extras
        if let Some(extra_args) = self.extra_args.as_ref() {
            if let Some(video_input_args) = extra_args.video_input.as_ref() {
                args.extend_from_slice(video_input_args);
            }
        }

        // declare audio input, if any
        if let Some(audio_input) = self.audio_input.as_ref() {
            // critical for older raspberries that cant keep up
            args.push("-thread_queue_size".to_string());
            args.push("256".to_string());

            // alsa or pulse
            args.push("-f".to_string());
            args.push(audio_input.device_type.to_string());

            args.push("-sample_rate".to_string());
            args.push(
                audio_input
                    .sample_rate
                    .unwrap_or(FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE)
                    .to_string(),
            );

            args.push("-sample_fmt".to_string());
            args.push(
                audio_input
                    .sample_format
                    .as_ref()
                    .map(|sf| sf.to_string())
                    .unwrap_or(FFMPEG_DEFAULT_AUDIO_SAMPLE_FORMAT.to_string()),
            );

            args.push("-channels".to_string());
            args.push(audio_input.channels.unwrap_or(1).to_string());

            args.push("-i".to_string());
            args.push(audio_input.device_node.clone());

            // args.push("-r:a".to_string());
            // args.push(
            //     audio_input
            //         .sample_rate
            //         .unwrap_or(FFMPEG_DEFAULT_AUDIO_SAMPLE_RATE)
            //         .to_string(),
            // );

            // inject extras
            if let Some(extra_args) = self.extra_args.as_ref() {
                if let Some(audio_input_args) = extra_args.audio_input.as_ref() {
                    args.extend_from_slice(audio_input_args);
                }
            }
        }

        // output configuration start

        // inject extras
        if let Some(extra_args) = self.extra_args.as_ref() {
            if let Some(output_args) = extra_args.output.as_ref() {
                args.extend_from_slice(output_args);
            }
        }

        // avoid transcoding at all costs
        args.push("-c:v".to_string());
        args.push("copy".to_string());

        if let Some(audio_input) = self.audio_input.as_ref() {
            // this is the most resource costly thing in the whole app...
            args.push("-c:a".to_string());
            args.push(
                audio_input
                    .output_format
                    .clone()
                    .unwrap_or_default()
                    .to_string(),
            );

            // audio bitrate
            args.push("-b:a".to_string());
            args.push(
                audio_input
                    .output_bitrate
                    .clone()
                    .unwrap_or(FFMPEG_DEFAULT_AUDIO_OUTPUT_BITRATE.to_string()),
            );

            // output streams mapping
            args.push("-map".to_string());
            args.push("0:0".to_string());
            args.push("-map".to_string());
            args.push("1:0".to_string());
        }

        // HLS live stream parameters
        args.push("-f".to_string());
        args.push("segment".to_string());

        // mpegts container
        args.push("-segment_format".to_string());
        args.push("mpegts".to_string());

        // mark it as live
        args.push("-segment_list_flags".to_string());
        args.push("live".to_string());

        // m3u8 file
        args.push("-segment_list_type".to_string());
        args.push("m3u8".to_string());

        // 4 seconds per segment
        args.push("-segment_time".to_string());
        args.push("4".to_string());

        // 8 segments per playlist
        args.push("-segment_list_size".to_string());
        args.push("8".to_string());

        // keep up to 10 files in the folder
        args.push("-segment_wrap".to_string());
        args.push("10".to_string());

        let stream_playlist = {
            let mut stream_dir = self.stream_dir.clone();
            stream_dir.push(FFMPEG_DEFAULT_STREAM_PLAYLIST_NAME);
            stream_dir
                .to_str()
                .expect("Failed to build stream playlist path")
                .to_string()
        };

        // playlist location
        args.push("-segment_list".to_string());
        args.push(stream_playlist);

        let stream_segment = {
            let mut stream_dir = self.stream_dir.clone();
            stream_dir.push(FFMPEG_DEFAULT_STREAM_SEGMENT_NAME_PATTERN);
            stream_dir
                .to_str()
                .expect("Failed to build stream segment path")
                .to_string()
        };

        // output segment file names pattern
        args.push(stream_segment);

        args
    }

    pub fn spawn(&self) -> Result<Child> {
        let args = self.build_ffmpeg_cmd_args();

        // info!("FFMPEG ARGS: {:?}", args);

        let ffmpeg = Command::new(FFMPEG_BIN)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn child process {}: {}", FFMPEG_BIN, e))?;

        Ok(ffmpeg)
    }
}

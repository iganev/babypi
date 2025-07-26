use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::ffmpeg::audio::FfmpegAudioSampleFormat;
use crate::telemetry::events::Event;
use anyhow::anyhow;
use anyhow::Result;
use libpulse_binding as pulse;
use libpulse_binding::sample::Format as PulseAudioSampleFormat;
use libpulse_simple_binding as simple;
use tokio::sync::broadcast::Sender;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

pub const AUDIO_MONITOR_BOOTSTRAP_RETRY: u8 = 10;
pub const AUDIO_MONITOR_DEFAULT_RMS_THRESHOLD: f32 = 0.1;

impl From<FfmpegAudioSampleFormat> for PulseAudioSampleFormat {
    fn from(value: FfmpegAudioSampleFormat) -> Self {
        match value {
            // FfmpegAudioSampleFormat::U8 => PulseAudioSampleFormat::U8,
            // FfmpegAudioSampleFormat::ALaw => PulseAudioSampleFormat::ALaw,
            // FfmpegAudioSampleFormat::ULaw => PulseAudioSampleFormat::ULaw,
            FfmpegAudioSampleFormat::S16le => PulseAudioSampleFormat::S16le,
            // FfmpegAudioSampleFormat::S16be => PulseAudioSampleFormat::S16be,
            FfmpegAudioSampleFormat::F32le => PulseAudioSampleFormat::F32le,
            // FfmpegAudioSampleFormat::F32be => PulseAudioSampleFormat::F32be,
            FfmpegAudioSampleFormat::S32le => PulseAudioSampleFormat::S32le,
            // FfmpegAudioSampleFormat::S32be => PulseAudioSampleFormat::S32be,
            // FfmpegAudioSampleFormat::S24le => PulseAudioSampleFormat::S24le,
            // FfmpegAudioSampleFormat::S24be => PulseAudioSampleFormat::S24be,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioMonitorContext {
    sample_format: PulseAudioSampleFormat,
    sample_rate: u32,
    channels: u8,
    device: Option<String>,

    rms_threshold: Option<f32>,
}

impl Default for AudioMonitorContext {
    fn default() -> Self {
        Self {
            sample_format: PulseAudioSampleFormat::S16le,
            sample_rate: 44_000,
            channels: 1,
            device: None,
            rms_threshold: Some(AUDIO_MONITOR_DEFAULT_RMS_THRESHOLD),
        }
    }
}

impl AudioMonitorContext {
    pub fn new(
        sample_format: PulseAudioSampleFormat,
        sample_rate: u32,
        channels: u8,
        device: Option<String>,
        rms_threshold: Option<f32>,
    ) -> Self {
        Self {
            sample_format,
            sample_rate,
            channels,
            device,
            rms_threshold,
        }
    }
}

#[derive(Debug)]
pub struct AudioMonitor {
    context: Arc<AudioMonitorContext>,
    handle: Option<JoinHandle<()>>,
    shutdown_signal: Arc<AtomicBool>,
    retry_count: u8,
    channel: Option<Sender<Event>>,
}

impl Default for AudioMonitor {
    fn default() -> Self {
        Self {
            context: Arc::new(AudioMonitorContext::default()),
            handle: None,
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            retry_count: 0,
            channel: None,
        }
    }
}

impl AudioMonitor {
    pub fn new(context: AudioMonitorContext, channel: Option<Sender<Event>>) -> Self {
        Self {
            context: Arc::new(context),
            channel,
            ..Default::default()
        }
    }

    /// Start monitor
    pub async fn start(&mut self) -> Result<()> {
        self.shutdown_signal.store(false, Ordering::SeqCst);

        loop {
            let handle = match self.start_inner() {
                Ok(handle) => handle,
                Err(e) => {
                    error!(
                        target = "audio_monitor",
                        "Error starting audio monitor: {}", e
                    );
                    self.retry_count += 1;

                    if self.retry_count > AUDIO_MONITOR_BOOTSTRAP_RETRY {
                        return Err(anyhow!(
                            "Failed to start audio monitor after {} retries",
                            AUDIO_MONITOR_BOOTSTRAP_RETRY
                        ));
                    }

                    tokio::time::sleep(Duration::from_secs(1)).await;

                    continue;
                }
            };

            self.handle = Some(handle);

            info!(target = "audio_monitor", "Audio monitor started");

            return Ok(());
        }
    }

    fn start_inner(&mut self) -> Result<JoinHandle<()>> {
        let context = self.context.clone();
        let channel = self.channel.clone();
        let shutdown = self.shutdown_signal.clone();

        Ok(tokio::task::spawn_blocking(move || {
            let buffer_size = match context.sample_rate {
                96_000 => 28_800,
                48_000 => 14_400,
                44_100 => 13_230,
                24_000 => 7_200,
                _ => {
                    error!(
                        target = "audio_monitor",
                        "Unsupported sample rate: {}", context.sample_rate
                    );
                    return;
                }
            };

            let pulse_connection = match simple::Simple::new(
                None,
                "babypi",
                pulse::stream::Direction::Record,
                context.device.as_deref(),
                "audio_monitor",
                &pulse::sample::Spec {
                    format: context.sample_format,
                    channels: context.channels,
                    rate: context.sample_rate,
                },
                None,
                None,
            ) {
                Ok(pc) => pc,
                Err(e) => {
                    error!(
                        target = "audio_monitor",
                        "Error connecting to pulseaudio: {}", e
                    );
                    return;
                }
            };

            match context.sample_format {
                PulseAudioSampleFormat::S16le => {
                    let mut buffer = vec![0i16; buffer_size]; // 300ms
                    let mut normalized_buffer = vec![0f32; buffer_size];

                    while !shutdown.load(Ordering::SeqCst) {
                        if let Err(e) = pulse_connection.read(i16_to_u8_slice(&mut buffer)) {
                            error!(
                                target = "audio_monitor",
                                "Error reading from pulseaudio stream: {}", e
                            );
                            return;
                        }

                        buffer
                            .iter()
                            .map(|sample| *sample as f32 / 32768.0)
                            .zip(normalized_buffer.iter_mut())
                            .for_each(|(b, df)| *df = b);

                        let rms = calculate_rms(&normalized_buffer);

                        if context.rms_threshold.is_some_and(|rms_t| rms > rms_t) {
                            debug!(target = "audio_monitor", "RMS = {}; TRIGGER = true", rms);

                            if let Some(channel) = &channel {
                                let _ = channel.send(Event::AudioMonitor { rms });
                            }
                        } else {
                            debug!(target = "audio_monitor", "RMS = {};  TRIGGER = false", rms);
                        }
                    }
                }
                PulseAudioSampleFormat::F32le => {
                    let mut buffer = vec![0f32; buffer_size]; // 300ms

                    while !shutdown.load(Ordering::SeqCst) {
                        if let Err(e) = pulse_connection.read(f32_to_u8_slice(&mut buffer)) {
                            error!(
                                target = "audio_monitor",
                                "Error reading from pulseaudio stream: {}", e
                            );
                            return;
                        }

                        let rms = calculate_rms(&buffer);

                        if context.rms_threshold.is_some_and(|rms_t| rms > rms_t) {
                            debug!(target = "audio_monitor", "RMS = {}; TRIGGER = true", rms);

                            if let Some(channel) = &channel {
                                let _ = channel.send(Event::AudioMonitor { rms });
                            }
                        } else {
                            debug!(target = "audio_monitor", "RMS = {};  TRIGGER = false", rms);
                        }
                    }
                }
                PulseAudioSampleFormat::S32le => {
                    let mut buffer = vec![0i32; buffer_size]; // 300ms
                    let mut normalized_buffer = vec![0f32; buffer_size];

                    while !shutdown.load(Ordering::SeqCst) {
                        if let Err(e) = pulse_connection.read(i32_to_u8_slice(&mut buffer)) {
                            error!(
                                target = "audio_monitor",
                                "Error reading from pulseaudio stream: {}", e
                            );
                            return;
                        }

                        buffer
                            .iter()
                            .map(|sample| *sample as f32 / 32768.0)
                            .zip(normalized_buffer.iter_mut())
                            .for_each(|(b, df)| *df = b);

                        let rms = calculate_rms(&normalized_buffer);

                        if context.rms_threshold.is_some_and(|rms_t| rms > rms_t) {
                            debug!(target = "audio_monitor", "RMS = {}; TRIGGER = true", rms);

                            if let Some(channel) = &channel {
                                let _ = channel.send(Event::AudioMonitor { rms });
                            }
                        } else {
                            debug!(target = "audio_monitor", "RMS = {};  TRIGGER = false", rms);
                        }
                    }
                }
                _ => {
                    error!(
                        target = "audio_monitor",
                        "Unsupported sample format: {:?}", context.sample_format
                    );
                }
            }
        }))
    }

    /// Stop monitor
    pub async fn stop(&mut self) {
        self.shutdown_signal.store(true, Ordering::SeqCst);

        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }

        info!(target = "audio_monitor", "Audio monitor stopped");
    }

    /// Are we monitoring?
    pub fn is_running(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }
}

/// Cast `[i16]` as `[u8]`
fn i16_to_u8_slice(slice: &mut [i16]) -> &mut [u8] {
    let byte_len = 2 * slice.len();
    unsafe { std::slice::from_raw_parts_mut(slice.as_mut_ptr().cast::<u8>(), byte_len) }
}

/// Cast `[f32]` as `[u8]`
fn f32_to_u8_slice(slice: &mut [f32]) -> &mut [u8] {
    let byte_len = 4 * slice.len();
    unsafe { std::slice::from_raw_parts_mut(slice.as_mut_ptr().cast::<u8>(), byte_len) }
}

/// Cast `[i32]` as `[u8]`
fn i32_to_u8_slice(slice: &mut [i32]) -> &mut [u8] {
    let byte_len = 4 * slice.len();
    unsafe { std::slice::from_raw_parts_mut(slice.as_mut_ptr().cast::<u8>(), byte_len) }
}

/// Calculate samples RMS
fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    // Sum the squares of all samples
    let sum_of_squares: f32 = samples.iter().map(|sample| sample * sample).sum();

    // Calculate the mean of squares
    let mean_of_squares = sum_of_squares / samples.len() as f32;

    // Return the square root of the mean
    mean_of_squares.sqrt()
}

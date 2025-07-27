use std::sync::Arc;
use std::time::Duration;

use crate::ffmpeg::FFMPEG_BIN;
use crate::rpicam::RPICAM_BIN;
use crate::telemetry::events::EventDispatcher;
use crate::{ffmpeg::Ffmpeg, process_control::ProcessControl, rpicam::Rpicam};
use anyhow::anyhow;
use anyhow::Result;
use image::RgbImage;
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;
use openh264::nal_units;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

pub const LIVE_STREAM_BOOTSTRAP_RETRY: u8 = 10;

#[derive(Debug, Default)]
struct LiveStreamState {
    rpicam_process: Option<ProcessControl>,
    ffmpeg_process: Option<ProcessControl>,

    handle_pipe: Option<JoinHandle<()>>,
    handle_watch: Option<JoinHandle<()>>,

    running: bool,
    retry_count: u8,
}

impl LiveStreamState {
    pub async fn start(
        &mut self,
        rpicam: &Rpicam,
        ffmpeg: &Ffmpeg,
        events: EventDispatcher,
    ) -> Result<()> {
        let mut rpicam_child = rpicam.spawn()?;
        let rpicam_stdout = rpicam_child.stdout.take().ok_or_else(|| {
            anyhow!(
                "Failed to capture child process output for `{}`",
                RPICAM_BIN
            )
        })?;
        let rpicam_process = ProcessControl::new(RPICAM_BIN, rpicam_child)?;

        info!(
            target = "live_stream",
            "Bootstrapped `{}` for live streaming", RPICAM_BIN
        );

        let mut ffmpeg_child = ffmpeg.spawn()?;
        let ffmpeg_stdin = ffmpeg_child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to open child process input for `{}`", FFMPEG_BIN))?;
        let ffmpeg_process = ProcessControl::new(FFMPEG_BIN, ffmpeg_child)?;

        info!(
            target = "live_stream",
            "Bootstrapped `{}` for live streaming", FFMPEG_BIN
        );

        let handle_pipe = tapped_io_pipe(rpicam_stdout, ffmpeg_stdin, events);

        info!(target = "live_stream", "Connected IO pipe");

        self.rpicam_process = Some(rpicam_process);
        self.ffmpeg_process = Some(ffmpeg_process);
        self.handle_pipe = Some(handle_pipe);

        self.running = true;

        Ok(())
    }

    pub async fn reset(&mut self) {
        self.stop().await;
        self.retry_count = 0;
    }

    pub async fn stop(&mut self) {
        self.running = false;

        if let Some(handle_pipe) = self.handle_pipe.take() {
            handle_pipe.abort();
        }

        if let Some(handle_watch) = self.handle_watch.take() {
            handle_watch.abort();
        }

        if let Some(mut ffmpeg_process) = self.ffmpeg_process.take() {
            if let Err(e) = ffmpeg_process.stop() {
                error!(
                    target = "live_stream",
                    "Error while stopping `{}`: {}", FFMPEG_BIN, e
                );
            }
        }

        if let Some(mut rpicam_process) = self.rpicam_process.take() {
            if let Err(e) = rpicam_process.stop() {
                error!(
                    target = "live_stream",
                    "Error while stopping `{}`: {}", RPICAM_BIN, e
                );
            }
        }
    }

    pub fn retry_increment(&mut self) -> u8 {
        self.retry_count += 1;

        self.retry_count
    }

    pub fn is_running(&self) -> bool {
        self.running
    }
}

#[derive(Debug)]
pub struct LiveStream {
    rpicam: Arc<Rpicam>,
    ffmpeg: Arc<Ffmpeg>,
    state: Arc<RwLock<LiveStreamState>>,
    watchdog: Arc<RwLock<Option<JoinHandle<()>>>>,
    events: EventDispatcher,
}

impl LiveStream {
    pub fn new(rpicam: Rpicam, ffmpeg: Ffmpeg, events: EventDispatcher) -> Self {
        Self {
            rpicam: Arc::new(rpicam),
            ffmpeg: Arc::new(ffmpeg),
            state: Arc::new(RwLock::new(LiveStreamState::default())),
            watchdog: Arc::new(RwLock::new(None)),
            events,
        }
    }

    /// Start streaming
    pub async fn start(&self) {
        let state_ref = self.state.clone();
        let rpicam_ref = self.rpicam.clone();
        let ffmpeg_ref = self.ffmpeg.clone();
        let events = self.events.clone();

        let watchdog = tokio::spawn(async move {
            loop {
                let state_lock = state_ref.read().await;
                let is_running = state_lock.running;
                let retry_count = state_lock.retry_count;
                drop(state_lock);

                if !is_running {
                    if retry_count < LIVE_STREAM_BOOTSTRAP_RETRY {
                        let mut state_lock = state_ref.write().await;
                        state_lock.retry_increment();

                        if let Err(e) = state_lock
                            .start(&rpicam_ref, &ffmpeg_ref, events.clone())
                            .await
                        {
                            error!(
                                target = "live_stream",
                                "Error while starting live stream: {}", e
                            );
                        } else {
                            // set up watch task

                            let mut watch_rpicam = if let Some(watch_rpicam) =
                                state_lock.rpicam_process.as_mut().and_then(|p| p.exit_rx())
                            {
                                watch_rpicam
                            } else {
                                error!(
                                    target = "live_stream",
                                    "Failed to get watch receiver for `{}`", RPICAM_BIN
                                );

                                drop(state_lock);

                                continue;
                            };

                            let mut watch_ffmpeg = if let Some(watch_ffmpeg) =
                                state_lock.ffmpeg_process.as_mut().and_then(|p| p.exit_rx())
                            {
                                watch_ffmpeg
                            } else {
                                error!(
                                    target = "live_stream",
                                    "Failed to get watch receiver for `{}`", FFMPEG_BIN
                                );

                                drop(state_lock);

                                continue;
                            };

                            let state_ref_watch = state_ref.clone();

                            let handle_watch = tokio::spawn(async move {
                                tokio::select! {
                                    r = &mut watch_rpicam => {
                                        match r {
                                            Ok(exit_code) => {
                                                warn!(target = "live_stream", "Process `{}` exit: {}", RPICAM_BIN, exit_code);
                                            }
                                            Err(e) => {
                                                error!(target = "live_stream", "Process `{}` watch error: {}", RPICAM_BIN, e);
                                            }
                                        }
                                    }
                                    r = &mut watch_ffmpeg => {
                                        match r {
                                            Ok(exit_code) => {
                                                warn!(target = "live_stream", "Process `{}` exit: {}", FFMPEG_BIN, exit_code);
                                            }
                                            Err(e) => {
                                                error!(target = "live_stream", "Process `{}` watch error: {}", FFMPEG_BIN, e);
                                            }
                                        }
                                    }
                                    else => {
                                        error!(target = "live_stream", "Both `{}` and `{}` seem to have exited prematurely...", RPICAM_BIN, FFMPEG_BIN);
                                    }
                                }

                                state_ref_watch.write().await.stop().await;
                            });

                            state_lock.handle_watch = Some(handle_watch);
                        }

                        drop(state_lock);
                    } else {
                        error!(target = "live_stream", "Too many retries: {}", retry_count);

                        break;
                    }
                }

                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });

        let mut watchdog_lock = self.watchdog.write().await;
        *watchdog_lock = Some(watchdog);
        drop(watchdog_lock);
    }

    /// Stop streaming and reset state
    pub async fn stop(&self) {
        let mut watchdog_lock = self.watchdog.write().await;
        if let Some(watchdog) = watchdog_lock.take() {
            info!(target = "live_stream", "Stopping stream watchdog");
            watchdog.abort();
        }
        drop(watchdog_lock);

        self.state.write().await.reset().await;
    }

    /// Are we live?
    pub async fn is_running(&self) -> bool {
        self.state.read().await.is_running()
    }
}

#[allow(dead_code)]
/// OG simple IO pipe
fn simple_io_pipe(mut rpicam_stdout: ChildStdout, mut ffmpeg_stdin: ChildStdin) -> JoinHandle<()> {
    tokio::spawn(async move {
        tokio::io::copy(&mut rpicam_stdout, &mut ffmpeg_stdin)
            .await
            .ok();
        error!(target = "live_stream", "Ran out of buffer to move around");
    })
}

#[allow(dead_code)]
fn tapped_io_pipe(
    mut rpicam_stdout: ChildStdout,
    mut ffmpeg_stdin: ChildStdin,
    events: EventDispatcher,
) -> JoinHandle<()> {
    let events_tx = events.get_sender();
    let events_rx = events.get_receiver();

    tokio::spawn(async move {
        let (tx, mut rx_pipe) = broadcast::channel(10);
        let mut rx_tap = tx.subscribe();

        let reader_handle = tokio::spawn(async move {
            let mut buffer = [0u8; 8192];
            loop {
                match rpicam_stdout.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = buffer[..n].to_vec();
                        if tx.send(data).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            error!(target = "live_stream", "Ran out of buffer to move around");
        });

        let pipe_handle = tokio::spawn(async move {
            while let Ok(data) = rx_pipe.recv().await {
                if ffmpeg_stdin.write_all(&data).await.is_err() {
                    break;
                }
            }
        });

        // let tap_handle = tokio::spawn(async move {
        //     let mut timer = tokio::time::interval(Duration::from_secs(60)); // TODO
        //     let mut buffer = Vec::new();
        //     let buffer_target = (128 * 1024) as usize; // TODO

        //     'outer_loop: loop {
        //         timer.tick().await;

        //         loop {
        //             match rx_tap.recv().await {
        //                 Ok(data) => {
        //                     buffer.extend_from_slice(&data);

        //                     if buffer.len() >= buffer_target {
        //                         let _ =
        //                             events_tx.send(crate::telemetry::events::Event::RawFrameData {
        //                                 data: buffer.clone(),
        //                             });

        //                         debug!("Sending {} bytes raw frame data event", buffer.len());

        //                         buffer.clear();
        //                         break;
        //                     }
        //                 }
        //                 Err(RecvError::Lagged(_)) => {}
        //                 Err(RecvError::Closed) => break 'outer_loop,
        //             }
        //         }
        //     }
        // });

        let tap_handle = tokio::spawn(async move {
            let mut buffer = Vec::new();

            let mut events_rx = events_rx.resubscribe();

            'outer_loop: while let Ok(event) = events_rx.recv().await {
                if let crate::telemetry::events::Event::SnapshotRequest = event {
                    let mut decoder = Decoder::new().expect("Unable to open h264 decoder");

                    info!("Received snapshot request");

                    loop {
                        match rx_tap.recv().await {
                            Ok(data) => {
                                buffer.extend_from_slice(&data);

                                info!("Collecting raw frames data");

                                let mut img_data = Vec::new();
                                let mut w: u32 = 0;
                                let mut h: u32 = 0;

                                for packet in nal_units(&buffer) {
                                    if let Ok(Some(frame)) = decoder.decode(packet) {
                                        img_data =
                                            vec![
                                                0;
                                                frame.dimensions().0 * frame.dimensions().1 * 3
                                            ];
                                        w = frame.dimensions().0 as u32;
                                        h = frame.dimensions().1 as u32;
                                        frame.write_rgb8(&mut img_data);
                                        info!("Parsed a valid frame");
                                        break;
                                    }
                                }

                                if !img_data.is_empty() && w > 0 && h > 0 {
                                    if let Some(img) = RgbImage::from_raw(w, h, img_data) {
                                        let _ = events_tx.send(
                                            crate::telemetry::events::Event::SnapshotData {
                                                data: img,
                                            },
                                        );

                                        info!("Sending snapshot data");

                                        buffer.clear();
                                        break;
                                    }
                                }
                            }
                            Err(RecvError::Lagged(_)) => {}
                            Err(RecvError::Closed) => break 'outer_loop,
                        }
                    }
                }
            }
        });

        let _ = tokio::join!(reader_handle, pipe_handle, tap_handle);
    })
}

use std::sync::Arc;
use std::time::Duration;

use crate::ffmpeg::FFMPEG_BIN;
use crate::rpicam::RPICAM_BIN;
use crate::{ffmpeg::Ffmpeg, process_control::ProcessControl, rpicam::Rpicam};
use anyhow::anyhow;
use anyhow::Result;
use tokio::sync::RwLock;
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
    pub async fn start(&mut self, rpicam: &Rpicam, ffmpeg: &Ffmpeg) -> Result<()> {
        let mut rpicam_child = rpicam.spawn()?;
        let mut rpicam_stdout = rpicam_child.stdout.take().ok_or_else(|| {
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
        let mut ffmpeg_stdin = ffmpeg_child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to open child process input for `{}`", FFMPEG_BIN))?;
        let ffmpeg_process = ProcessControl::new(FFMPEG_BIN, ffmpeg_child)?;

        info!(
            target = "live_stream",
            "Bootstrapped `{}` for live streaming", FFMPEG_BIN
        );

        let handle_pipe = tokio::spawn(async move {
            tokio::io::copy(&mut rpicam_stdout, &mut ffmpeg_stdin)
                .await
                .ok();
            error!(target = "live_stream", "Ran out of buffer to move around");
        });

        info!(target = "live_stream", "Connected IO pipe");

        self.rpicam_process = Some(rpicam_process);
        self.ffmpeg_process = Some(ffmpeg_process);
        self.handle_pipe = Some(handle_pipe);

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

        if let Some(ffmpeg_process) = self.ffmpeg_process.take() {
            if let Err(e) = ffmpeg_process.stop() {
                error!(
                    target = "live_stream",
                    "Error while stopping `{}`: {}", FFMPEG_BIN, e
                );
            }
        }

        if let Some(rpicam_process) = self.rpicam_process.take() {
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
}

impl LiveStream {
    pub fn new(rpicam: Rpicam, ffmpeg: Ffmpeg) -> Self {
        Self {
            rpicam: Arc::new(rpicam),
            ffmpeg: Arc::new(ffmpeg),
            state: Arc::new(RwLock::new(LiveStreamState::default())),
            watchdog: Arc::new(RwLock::new(None)),
        }
    }

    /// Start streaming
    pub async fn start(&self) {
        let state_ref = self.state.clone();
        let rpicam_ref = self.rpicam.clone();
        let ffmpeg_ref = self.ffmpeg.clone();

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

                        if let Err(e) = state_lock.start(&rpicam_ref, &ffmpeg_ref).await {
                            error!(
                                target = "live_stream",
                                "Error while starting live stream: {}", e
                            );
                        } else {
                            // set up watch task

                            let mut watch_rpicam = if let Some(rpicam_process) =
                                state_lock.rpicam_process.as_mut()
                            {
                                if let Some(watch_rpicam) = rpicam_process.exit_rx() {
                                    watch_rpicam
                                } else {
                                    error!(
                                        target = "live_stream",
                                        "Failed to get watch receiver for `{}`", RPICAM_BIN
                                    );

                                    drop(state_lock);

                                    continue;
                                }
                            } else {
                                error!(
                                    target = "live_stream",
                                    "Failed to get process control reference for `{}`", RPICAM_BIN
                                );

                                drop(state_lock);

                                continue;
                            };

                            let mut watch_ffmpeg = if let Some(ffmpeg_process) =
                                state_lock.ffmpeg_process.as_mut()
                            {
                                if let Some(watch_ffmpeg) = ffmpeg_process.exit_rx() {
                                    watch_ffmpeg
                                } else {
                                    error!(
                                        target = "live_stream",
                                        "Failed to get watch receiver for `{}`", FFMPEG_BIN
                                    );

                                    drop(state_lock);

                                    continue;
                                }
                            } else {
                                error!(
                                    target = "live_stream",
                                    "Failed to get process control reference for `{}`", FFMPEG_BIN
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

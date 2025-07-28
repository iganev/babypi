#![allow(dead_code)]
use crate::serde_stuff::float_precision_two;
use image::{ImageBuffer, Rgb};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio::sync::RwLock;
use tracing::log::log;
use tracing::log::Level;

pub const EVENT_DISPATCHER_CAPACITY: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Service {
    VideoStream,
    WebServer,
    AudioMonitor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    Disabled,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Test(String),

    ServiceStatus {
        service: Service,
        status: Status,
    },

    SnapshotRequest,

    SnapshotData {
        #[serde(skip)]
        data: ImageBuffer<Rgb<u8>, Vec<u8>>,
    },

    SnapshotUpdated {
        filesize: u64,
        width: u32,
        height: u32,
    },

    AudioMonitor {
        #[serde(with = "float_precision_two")]
        rms: f32,
    },
}

#[derive(Debug)]
pub struct EventDispatcher {
    tx: Sender<Event>,
    rx: RwLock<Receiver<Event>>,
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl EventDispatcher {
    /// Create a new event dispatcher
    pub fn new() -> Self {
        let (tx, rx) = broadcast::channel::<Event>(EVENT_DISPATCHER_CAPACITY);

        Self {
            tx,
            rx: RwLock::new(rx),
        }
    }

    /// Get a sender instance to send events
    pub fn get_sender(&self) -> Sender<Event> {
        self.tx.clone()
    }

    /// Get a receiver instance to receive events
    pub fn get_receiver(&self) -> Receiver<Event> {
        self.tx.subscribe()
    }

    /// Get number of active receivers
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Check if event queue is empty
    pub fn is_empty(&self) -> bool {
        self.tx.is_empty()
    }

    /// Get number of queued events
    pub fn len(&self) -> usize {
        self.tx.len()
    }

    /// Send event. Returns number of receivers that received the event
    pub fn send(&self, event: Event) -> usize {
        self.tx.send(event).ok().unwrap_or(0)
    }

    /// Check if sender is linked to this event dispatcher channel
    pub fn same_sender(&self, other: &Sender<Event>) -> bool {
        self.tx.same_channel(other)
    }

    /// Check if receiver is linked to this event dispatcher channel
    pub async fn same_receiver(&self, other: &Receiver<Event>) -> bool {
        self.rx.read().await.same_channel(other)
    }

    /// Drain internal receiver event queue
    pub async fn drain_backlog(&self) -> Vec<Event> {
        let mut results = Vec::new();

        let mut rx = self.rx.write().await;

        if !rx.is_empty() {
            loop {
                let try_event = rx.try_recv();

                match try_event {
                    Ok(event) => results.push(event),
                    Err(err) => match err {
                        broadcast::error::TryRecvError::Empty => break,
                        broadcast::error::TryRecvError::Closed => break,
                        broadcast::error::TryRecvError::Lagged(_) => {}
                    },
                }
            }
        }

        results
    }
}

impl Clone for EventDispatcher {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            rx: RwLock::new(self.tx.subscribe()),
        }
    }
}

pub fn spawn_logger(event_dispatcher: &EventDispatcher) {
    let mut rx = event_dispatcher.get_receiver();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    log!(target: "event_dispatcher", Level::Info, "Event: {:?}", event);
                }
                Err(err) => match err {
                    tokio::sync::broadcast::error::RecvError::Closed => {
                        log!(target: "event_dispatcher", Level::Info, "Event dispatcher terminated");
                        break;
                    }
                    tokio::sync::broadcast::error::RecvError::Lagged(n) => {
                        log!(target: "event_dispatcher", Level::Info, "Lagging behind with {} events", n);
                    }
                },
            }
        }
    });
}

use actix::prelude::*;
use actix_web::Result;
use actix_web_actors::ws;
use chrono::Utc;
use serde_json::json;
use std::time::{Duration, Instant};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::{debug, error, info};

use crate::telemetry::events::EventDispatcher;

#[derive(Clone, Debug)]
pub struct TelemetryMessage {
    pub event_type: String,
    pub data: serde_json::Value,
}

pub struct TelemetryWebsocketSession {
    hb: Instant,
    events: EventDispatcher,
}

impl TelemetryWebsocketSession {
    pub fn new(events: &EventDispatcher) -> Self {
        Self {
            hb: Instant::now(),
            events: events.clone(),
        }
    }

    fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(Duration::from_secs(5), |act, ctx| {
            if Instant::now().duration_since(act.hb) > Duration::from_secs(10) {
                ctx.stop();
                info!(target = "telemetry", "Connection timeout");
                return;
            }

            ctx.ping(b"hi");
        });
    }

    fn start_broadcast_listener(&mut self, ctx: &mut <Self as Actor>::Context) {
        ctx.add_stream(BroadcastStream::new(self.events.get_receiver()).map(
            |result| match result {
                Ok(event) => TelemetryMessage {
                    event_type: "telemetry".to_string(),
                    data: serde_json::to_value(event).unwrap_or(serde_json::Value::Null),
                },
                Err(e) => TelemetryMessage {
                    event_type: "error".to_string(),
                    data: json!({"error": format!("{}", e)}),
                },
            },
        ));
    }

    fn send_json_event(
        &self,
        ctx: &mut <Self as Actor>::Context,
        event_type: &str,
        data: serde_json::Value,
    ) {
        let event = json!({
            "type": event_type,
            "timestamp": Utc::now().to_rfc3339(),
            "data": data
        });
        ctx.text(event.to_string());
    }
}

impl Actor for TelemetryWebsocketSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);

        self.send_json_event(ctx, "connected", json!({"message": "Telemetry connected"}));

        self.start_broadcast_listener(ctx);

        // ctx.run_interval(Duration::from_secs(5), |act, ctx| {
        //     act.send_json_event(ctx, "heartbeat", json!({"status": "active"}));
        // });
    }
}

impl StreamHandler<TelemetryMessage> for TelemetryWebsocketSession {
    fn handle(&mut self, msg: TelemetryMessage, ctx: &mut Self::Context) {
        self.send_json_event(ctx, &msg.event_type, msg.data);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for TelemetryWebsocketSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                // debug!(target = "telemetry", "ping: {:?}", msg);
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                // debug!(target = "telemetry", "pong: {:?}", msg);
                self.hb = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                debug!(target = "telemetry", "Received text: {}", text);
            }
            Ok(ws::Message::Binary(bin)) => {
                debug!(target = "telemetry", "Received binary: {:#?}", bin);
            }
            Ok(ws::Message::Close(reason)) => {
                info!(target = "telemetry", "Connection closed: {:?}", reason);

                ctx.close(reason);
                ctx.stop();
            }
            _ => {
                error!(target = "telemetry", "unknown message: {:?}", msg);

                ctx.stop()
            }
        }
    }
}

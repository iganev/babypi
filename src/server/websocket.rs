use actix_web::{web, HttpRequest, HttpResponse, Result};
use actix_web_actors::ws;

use crate::{
    server::websocket::telemetry::TelemetryWebsocketSession, telemetry::events::EventDispatcher,
};

pub mod telemetry;

/// Telemetry WebSocket endpoint handler
pub async fn ws_handler_telemetry(
    req: HttpRequest,
    stream: web::Payload,
    events: web::Data<EventDispatcher>,
) -> Result<HttpResponse> {
    ws::start(TelemetryWebsocketSession::new(&events), &req, stream)
}

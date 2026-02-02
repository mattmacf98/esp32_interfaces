extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::Ordering;

use embassy_net::Stack;
use embassy_time::Duration;
use esp_alloc as _;
use picoserve::{AppBuilder, AppRouter, Router, response::IntoResponse, routing};

#[derive(serde::Deserialize)]
struct PinRequest {
    pin_writes: Vec<PinWriteItem>,
}

#[derive(serde::Deserialize)]
struct PinWriteItem {
    pin_num: u8,
    state: u8,
}

#[derive(serde::Serialize)]
struct PinWriteResponse {
    success: bool,
}

#[derive(serde::Deserialize)]
struct PinReadRequest {
    pin_reads: Vec<u8>,
}

#[derive(serde::Serialize)]
struct PinReadItem {
    pin_num: u8,
    state: i32,
}

#[derive(serde::Serialize)]
struct PinReadResponse {
    pin_reads: Vec<PinReadItem>,
    success: bool,
}

pub struct Application;

impl AppBuilder for Application {
    type PathRouter = impl routing::PathRouter;

    fn build_app(self) -> picoserve::Router<Self::PathRouter> {
        picoserve::Router::new()
            .route("/write-pins", routing::post(write_pins_handler))
            .route("/read-pins", routing::post(read_pins_handler))
    }
}

async fn write_pins_handler(input: picoserve::extract::Json<PinRequest>) -> impl IntoResponse {
    for item in &input.0.pin_writes {
        match item.pin_num {
            14 => crate::pin::GPIO14_STATE.store(item.state as i32, Ordering::Relaxed),
            26 => crate::pin::GPIO26_STATE.store(item.state as i32, Ordering::Relaxed),
            25 => crate::pin::GPIO25_STATE.store(item.state as i32, Ordering::Relaxed),
            33 => crate::pin::GPIO33_STATE.store(item.state as i32, Ordering::Relaxed),
            _ => {}
        }
    }

    picoserve::response::Json(PinWriteResponse { success: true })
}

async fn read_pins_handler(input: picoserve::extract::Json<PinReadRequest>) -> impl IntoResponse {
    let mut pin_reads: Vec<PinReadItem> = Vec::new();
    for pin_num in input.0.pin_reads {
        let state = match pin_num {
            14 => crate::pin::GPIO14_STATE.load(Ordering::Relaxed),
            26 => crate::pin::GPIO26_STATE.load(Ordering::Relaxed),
            25 => crate::pin::GPIO25_STATE.load(Ordering::Relaxed),
            33 => crate::pin::GPIO33_STATE.load(Ordering::Relaxed),
            32 => crate::pin::GPIO32_STATE.load(Ordering::Relaxed),
            35 => crate::pin::GPIO35_STATE.load(Ordering::Relaxed),
            _ => 0,
        };
        pin_reads.push(PinReadItem {
            pin_num,
            state: state as i32,
        });
    }
    picoserve::response::Json(PinReadResponse {
        pin_reads,
        success: true,
    })
}

pub const WEB_TASK_POOL_SIZE: usize = 2;

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn web_task(
    task_id: usize,
    stack: Stack<'static>,
    router: &'static AppRouter<Application>,
    config: &'static picoserve::Config<Duration>,
) -> ! {
    let port = 80;
    let mut tcp_rx_buffer = [0; 1024];
    let mut tcp_tx_buffer = [0; 1024];
    let mut http_buffer = [0; 2048];

    picoserve::Server::new(router, config, &mut http_buffer)
        .listen_and_serve(task_id, stack, port, &mut tcp_rx_buffer, &mut tcp_tx_buffer)
        .await
        .into_never()
}

pub struct WebApp {
    pub router: &'static Router<<Application as AppBuilder>::PathRouter>,
    pub config: &'static picoserve::Config<Duration>,
}

impl Default for WebApp {
    fn default() -> Self {
        let router = picoserve::make_static!(AppRouter<Application>, Application.build_app());

        let config = picoserve::make_static!(
            picoserve::Config<Duration>,
            picoserve::Config::new(picoserve::Timeouts {
                start_read_request: Some(Duration::from_secs(5)),
                read_request: Some(Duration::from_secs(1)),
                write: Some(Duration::from_secs(1)),
                persistent_start_read_request: Some(Duration::from_secs(1)),
            })
            .keep_connection_alive()
        );

        Self { router, config }
    }
}

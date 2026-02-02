#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use bt_hci::controller::ExternalController;
use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Output;
use esp_hal::gpio::OutputConfig;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::ble::controller::BleConnector;
use lib::pin::BasicWritePinTaskItem;

use trouble_host::prelude::*;
use {esp_backtrace as _, esp_println as _};

// Our module
use bluetooth_low_energy as lib;
use lib::ble;

extern crate alloc;
use alloc::vec::Vec;

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 1;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.1.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    let radio_init = esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller");
    // find more examples https://github.com/embassy-rs/trouble/tree/main/examples/esp32
    let transport = BleConnector::new(&radio_init, peripherals.BT, Default::default()).unwrap();
    let ble_controller = ExternalController::<_, 20>::new(transport);
    // let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
    //     HostResources::new();
    // let _stack = trouble_host::new(ble_controller, &mut resources);

    let mut gpio14 = Some(peripherals.GPIO14);
    let basic_write_pin_nums = [14];

    // Basic write pins
    let mut basic_write_pins: Vec<BasicWritePinTaskItem> =
        Vec::with_capacity(basic_write_pin_nums.len());
    for pin_num in basic_write_pin_nums {
        // Map pin number to actual peripheral - expand this for more pins
        let pin = match pin_num {
            14 => gpio14
                .take()
                .map(|p| Output::new(p, esp_hal::gpio::Level::Low, OutputConfig::default())),
            _ => None,
        };
        if let Some(pin) = pin {
            basic_write_pins.push(BasicWritePinTaskItem { pin_num, pin });
        }
    }

    let basic_write_items: &'static mut [BasicWritePinTaskItem] = basic_write_pins.leak();
    spawner.must_spawn(lib::pin::basic_write_pin_task(basic_write_items));
    ble::run(ble_controller).await;

    loop {
        Timer::after(Duration::from_secs(5)).await;
    }
}

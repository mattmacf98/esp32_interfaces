#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_radio::ble::controller::BleConnector;
use trouble_host::prelude::*;

use esp_hal::analog::adc::{AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{DriveMode, Input, InputConfig, Output, OutputConfig};
use esp_hal::ledc::channel::ChannelIFace;
use esp_hal::ledc::timer::TimerIFace;
use esp_hal::ledc::{HighSpeed, LSGlobalClkSource, Ledc, LowSpeed, channel, timer};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_println as _;

use lib::ble;
use lib::pin::{
    AdcReadPinTaskItem, BasicReadPinTaskItem, BasicWritePinTaskItem, PWMWritePinTaskItem,
};

use webserver_html as lib;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[derive(serde::Deserialize)]
struct Config {
    bluetooth_name: String,
    basic_write_pin_nums: Vec<u8>,
    pwm_write_pin_nums: Vec<u8>,
    basic_read_pin_nums: Vec<u8>,
    adc_read_pin_nums: Vec<u8>,
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.0.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 98767);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    let radio_init = &*lib::mk_static!(
        esp_radio::Controller<'static>,
        esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller")
    );
    let transport = BleConnector::new(&radio_init, peripherals.BT, Default::default()).unwrap();
    let ble_controller = ExternalController::<_, 64>::new(transport);

    let config_data = include_bytes!("../config.json");
    let config_string = String::from_utf8(config_data.to_vec()).unwrap();
    let (config, _len) = serde_json_core::from_str::<Config>(&config_string).unwrap();

    let basic_write_pin_nums = config.basic_write_pin_nums;
    let pwm_write_pin_nums = config.pwm_write_pin_nums;
    let basic_read_pin_nums = config.basic_read_pin_nums;
    let adc_read_pin_nums = config.adc_read_pin_nums;
    let bluetooth_name = config.bluetooth_name;

    // Wrap peripherals in Option so we can take them once in the loop
    let mut gpio14 = Some(peripherals.GPIO14);
    let mut gpio26 = Some(peripherals.GPIO26);
    let mut gpio25 = Some(peripherals.GPIO25);
    let mut gpio32 = Some(peripherals.GPIO32);
    let mut gpio35 = Some(peripherals.GPIO35);
    let mut gpio33 = Some(peripherals.GPIO33);

    // Basic write pins
    let mut basic_write_pins: Vec<BasicWritePinTaskItem> =
        Vec::with_capacity(basic_write_pin_nums.len());
    for pin_num in basic_write_pin_nums {
        // Map pin number to actual peripheral - expand this for more pins
        let pin = match pin_num {
            14 => gpio14
                .take()
                .map(|p| Output::new(p, esp_hal::gpio::Level::Low, OutputConfig::default())),
            26 => gpio26
                .take()
                .map(|p| Output::new(p, esp_hal::gpio::Level::Low, OutputConfig::default())),
            25 => gpio25
                .take()
                .map(|p| Output::new(p, esp_hal::gpio::Level::Low, OutputConfig::default())),
            33 => gpio33
                .take()
                .map(|p| Output::new(p, esp_hal::gpio::Level::Low, OutputConfig::default())),
            _ => None,
        };
        if let Some(pin) = pin {
            basic_write_pins.push(BasicWritePinTaskItem { pin_num, pin });
        }
    }

    // Basic read pins
    let mut basic_read_pins: Vec<BasicReadPinTaskItem> =
        Vec::with_capacity(basic_read_pin_nums.len());
    for pin_num in basic_read_pin_nums {
        let pin = match pin_num {
            14 => gpio14.take().map(|p| Input::new(p, InputConfig::default())),
            26 => gpio26.take().map(|p| Input::new(p, InputConfig::default())),
            25 => gpio25.take().map(|p| Input::new(p, InputConfig::default())),
            33 => gpio33.take().map(|p| Input::new(p, InputConfig::default())),
            _ => None,
        };
        if let Some(pin) = pin {
            basic_read_pins.push(BasicReadPinTaskItem { pin_num, pin });
        }
    }

    // PWM write pins
    let mut ledc = Ledc::new(peripherals.LEDC);
    let hstimer0 = lib::mk_static!(timer::Timer<'static, HighSpeed>, {
        let mut t = ledc.timer::<HighSpeed>(timer::Number::Timer0);
        t.configure(timer::config::Config {
            duty: timer::config::Duty::Duty12Bit,
            clock_source: timer::HSClockSource::APBClk,
            frequency: Rate::from_hz(50),
        })
        .unwrap();
        t
    });
    const CHANNELS: [channel::Number; 8] = [
        channel::Number::Channel0,
        channel::Number::Channel1,
        channel::Number::Channel2,
        channel::Number::Channel3,
        channel::Number::Channel4,
        channel::Number::Channel5,
        channel::Number::Channel6,
        channel::Number::Channel7,
    ];
    let mut channel_idx = 0;

    let mut pwm_write_pins: Vec<PWMWritePinTaskItem> = Vec::with_capacity(pwm_write_pin_nums.len());
    for pin_num in pwm_write_pin_nums {
        // Map pin number to actual peripheral - expand this for more pins
        let pin = match pin_num {
            14 => gpio14
                .take()
                .map(|p| Output::new(p, esp_hal::gpio::Level::Low, OutputConfig::default())),
            26 => gpio26
                .take()
                .map(|p| Output::new(p, esp_hal::gpio::Level::Low, OutputConfig::default())),
            25 => gpio25
                .take()
                .map(|p| Output::new(p, esp_hal::gpio::Level::Low, OutputConfig::default())),
            33 => gpio33
                .take()
                .map(|p| Output::new(p, esp_hal::gpio::Level::Low, OutputConfig::default())),
            _ => None,
        };
        if let Some(pin) = pin {
            let mut ch = ledc.channel(CHANNELS[channel_idx], pin);
            ch.configure(channel::config::Config {
                timer: hstimer0,
                duty_pct: 0,
                drive_mode: DriveMode::PushPull,
            })
            .unwrap();
            pwm_write_pins.push(PWMWritePinTaskItem {
                pin_num,
                pwm_channel: ch,
            });
            channel_idx += 1;
        }
    }

    let mut adc_config = AdcConfig::new();
    let mut adc_read_pins: Vec<AdcReadPinTaskItem> = Vec::with_capacity(adc_read_pin_nums.len());
    for pin_num in adc_read_pin_nums.clone() {
        let pin = match pin_num {
            32 => gpio32.take().map(|p| {
                let adc_pin = adc_config.enable_pin(p, Attenuation::_11dB);
                let adc_read_pin = AdcReadPinTaskItem {
                    pin_num,
                    gpio32: Some(adc_pin),
                    gpio35: None,
                };
                adc_read_pin
            }),
            35 => gpio35.take().map(|p| {
                let adc_pin = adc_config.enable_pin(p, Attenuation::_11dB);
                let adc_read_pin = AdcReadPinTaskItem {
                    pin_num,
                    gpio32: None,
                    gpio35: Some(adc_pin),
                };
                adc_read_pin
            }),
            _ => None,
        };
        if let Some(pin) = pin {
            adc_read_pins.push(pin);
        }
    }

    // Leak the Vec to get a 'static slice for the task (ok because we expect pins to live forever)
    let basic_write_items: &'static mut [BasicWritePinTaskItem] = basic_write_pins.leak();
    spawner.must_spawn(lib::pin::basic_write_pin_task(basic_write_items));
    let pwm_write_items: &'static mut [PWMWritePinTaskItem] = pwm_write_pins.leak();
    spawner.must_spawn(lib::pin::pwm_write_pin_task(pwm_write_items));
    let basic_read_items: &'static mut [BasicReadPinTaskItem] = basic_read_pins.leak();
    spawner.must_spawn(lib::pin::basic_read_pin_task(basic_read_items));
    let adc_read_items: &'static mut [AdcReadPinTaskItem] = adc_read_pins.leak();
    spawner.must_spawn(lib::pin::adc_read_pin_task(
        adc_read_items,
        peripherals.ADC1,
        adc_config,
    ));

    ble::run(ble_controller, &bluetooth_name, adc_read_pin_nums).await;

    let mut loop_count = 0;
    loop {
        info!("looping main task: {}", loop_count);
        loop_count += 1;
        Timer::after(Duration::from_secs(1)).await;
    }
}

use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering;

use embassy_time::Duration;
use embassy_time::Timer;
use embedded_hal::pwm::SetDutyCycle;
use esp_hal::analog::adc::Adc;
use esp_hal::analog::adc::AdcConfig;
use esp_hal::analog::adc::AdcPin;
use esp_hal::gpio::Input;
use esp_hal::gpio::{Level, Output};
use esp_hal::ledc::HighSpeed;
use esp_hal::ledc::channel::{Channel, ChannelIFace};
use esp_hal::peripherals::ADC1;
use esp_hal::peripherals::GPIO32;
use esp_hal::peripherals::GPIO35;

pub static GPIO14_STATE: AtomicU32 = AtomicU32::new(0);
pub static GPIO26_STATE: AtomicU32 = AtomicU32::new(0);
pub static GPIO25_STATE: AtomicU32 = AtomicU32::new(0);
pub static GPIO32_STATE: AtomicU32 = AtomicU32::new(0);
pub static GPIO35_STATE: AtomicU32 = AtomicU32::new(0);
pub static GPIO33_STATE: AtomicU32 = AtomicU32::new(0);
pub struct BasicWritePinTaskItem {
    pub pin_num: u8,
    pub pin: Output<'static>,
}

#[embassy_executor::task]
pub async fn basic_write_pin_task(items: &'static mut [BasicWritePinTaskItem]) {
    loop {
        for item in items.iter_mut() {
            let pin_num = item.pin_num;
            let state = match pin_num {
                14 => GPIO14_STATE.load(Ordering::Relaxed),
                26 => GPIO26_STATE.load(Ordering::Relaxed),
                25 => GPIO25_STATE.load(Ordering::Relaxed),
                33 => GPIO33_STATE.load(Ordering::Relaxed),
                _ => 0,
            };
            if state == 100 {
                item.pin.set_level(Level::High);
            } else {
                item.pin.set_level(Level::Low);
            }
        }

        Timer::after(Duration::from_millis(500)).await;
    }
}

pub struct PWMWritePinTaskItem {
    pub pin_num: u8,
    pub pwm_channel: Channel<'static, HighSpeed>,
}

#[embassy_executor::task]
pub async fn pwm_write_pin_task(items: &'static mut [PWMWritePinTaskItem]) {
    loop {
        for item in items.iter_mut() {
            let pin_num = item.pin_num;
            let state = match pin_num {
                14 => GPIO14_STATE.load(Ordering::Relaxed),
                26 => GPIO26_STATE.load(Ordering::Relaxed),
                25 => GPIO25_STATE.load(Ordering::Relaxed),
                33 => GPIO33_STATE.load(Ordering::Relaxed),
                _ => 0,
            };
            let max_duty = item.pwm_channel.max_duty_cycle() as u32;
            let duty = (state as u32 * max_duty) / 100;
            let _ = item.pwm_channel.set_duty_cycle(duty as u16);
        }

        Timer::after(Duration::from_millis(500)).await;
    }
}

pub struct BasicReadPinTaskItem {
    pub pin_num: u8,
    pub pin: Input<'static>,
}

#[embassy_executor::task]
pub async fn basic_read_pin_task(items: &'static mut [BasicReadPinTaskItem]) {
    loop {
        for item in items.iter_mut() {
            let state = item.pin.level();
            //TODO: test if this actually works
            let u8_state = match state {
                Level::Low => 0,
                Level::High => 100,
            };
            match item.pin_num {
                14 => GPIO14_STATE.store(u8_state, Ordering::Relaxed),
                26 => GPIO26_STATE.store(u8_state, Ordering::Relaxed),
                25 => GPIO25_STATE.store(u8_state, Ordering::Relaxed),
                33 => GPIO33_STATE.store(u8_state, Ordering::Relaxed),
                _ => {}
            }
        }

        Timer::after(Duration::from_millis(500)).await;
    }
}

pub struct AdcReadPinTaskItem {
    pub pin_num: u8,
    pub gpio35: Option<AdcPin<GPIO35<'static>, ADC1<'static>>>,
    pub gpio32: Option<AdcPin<GPIO32<'static>, ADC1<'static>>>,
}

#[embassy_executor::task]
pub async fn adc_read_pin_task(
    items: &'static mut [AdcReadPinTaskItem],
    adc1: ADC1<'static>,
    adc1_config: AdcConfig<ADC1<'static>>,
) {
    let mut adc = Adc::new(adc1, adc1_config);
    loop {
        for item in items.iter_mut() {
            let state = match item.pin_num {
                35 => {
                    if let Some(mut gpio35) = item.gpio35.take() {
                        let Ok(value): Result<u16, _> = nb::block!(adc.read_oneshot(&mut gpio35))
                        else {
                            // Put the pin back so it can be used next time
                            item.gpio35 = Some(gpio35);
                            continue;
                        };
                        // Put the pin back so it can be used in the next cycle
                        item.gpio35 = Some(gpio35);
                        value as u32
                    } else {
                        0
                    }
                }
                32 => {
                    if let Some(mut gpio32) = item.gpio32.take() {
                        let Ok(value): Result<u16, _> = nb::block!(adc.read_oneshot(&mut gpio32))
                        else {
                            // Put the pin back so it can be used next time
                            item.gpio32 = Some(gpio32);
                            continue;
                        };
                        // Put the pin back so it can be used in the next cycle
                        item.gpio32 = Some(gpio32);
                        value as u32
                    } else {
                        0
                    }
                }
                _ => 0,
            };

            match item.pin_num {
                35 => GPIO35_STATE.store(state, Ordering::Relaxed),
                32 => GPIO32_STATE.store(state, Ordering::Relaxed),
                _ => {}
            }
        }

        Timer::after(Duration::from_millis(500)).await;
    }
}

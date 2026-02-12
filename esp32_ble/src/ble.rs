use core::sync::atomic::Ordering;

use defmt::{info, warn};

use embassy_futures::join::join;
use embassy_futures::select::select;

extern crate alloc;
use alloc::vec::Vec;

use embassy_time::Timer;

// BLE:
use trouble_host::prelude::*;

/// Max number of connections
const CONNECTIONS_MAX: usize = 1;

/// Max number of L2CAP channels.
const L2CAP_CHANNELS_MAX: usize = 1;

#[derive(serde::Deserialize)]
struct PinRequest {
    pin_writes: Vec<PinWriteItem>, // Fixed capacity of 8 pin writes max
}

#[derive(serde::Deserialize)]
struct PinWriteItem {
    pin_num: u8,
    state: u8,
}

// GATT Server definition
#[gatt_server]
struct Server {
    pin_service: PinService,
}

/// Pin service
#[gatt_service(uuid = "a9c81b72-0f7a-4c59-b0a8-425e3bcf0a0e")]
struct PinService {
    #[characteristic(uuid = "13c0ef83-09bd-4767-97cb-ee46224ae6db", read)]
    pin_data_output: [u8; 32],

    #[characteristic(uuid = "01037594-1bbb-4490-aa4d-f6d333b42e16", read)]
    adc_data_output: [u8; 32],

    #[characteristic(uuid = "c79b2ca7-f39d-4060-8168-816fa26737b7", read, write)]
    pin_data_input: [u8; 32],
}

/// Run the BLE stack.
///
pub async fn run<C>(controller: C, bluetooth_name: &str, adc_read_pin_nums: Vec<u8>)
where
    C: Controller,
{
    // Using a fixed "random" address can be useful for testing. In real scenarios, one would
    // use e.g. the MAC 6 byte array as the address (how to get that varies by the platform).
    let address: Address = Address::random([0xff, 0x8f, 0x1a, 0x05, 0xe4, 0xff]);
    info!("Our address = {:?}", defmt::Debug2Format(&address));

    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let stack = trouble_host::new(controller, &mut resources).set_random_address(address);
    let Host {
        mut peripheral,
        runner,
        ..
    } = stack.build();

    info!("Starting advertising and GATT service");
    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: bluetooth_name,
        appearance: &appearance::power_device::GENERIC_POWER_DEVICE,
    }))
    .unwrap();

    let _ = join(ble_task(runner), async {
        loop {
            match advertise(bluetooth_name, &mut peripheral, &server).await {
                Ok(conn) => {
                    // set up tasks when the connection is established to a central, so they don't run when no one is connected.
                    let a = gatt_events_task(&server, &conn);
                    let b = custom_task(&server, &conn, &stack, adc_read_pin_nums.clone());
                    // run until any task ends (usually because the connection has been closed),
                    // then return to advertising state.
                    select(a, b).await;
                }
                Err(e) => {
                    let e = defmt::Debug2Format(&e);
                    panic!("[adv] error: {:?}", e);
                }
            }
        }
    })
    .await;
}

/// This is a background task that is required to run forever alongside any other BLE tasks.
///
/// ## Alternative
///
/// If you didn't require this to be generic for your application, you could statically spawn this with i.e.
///
/// ```rust,ignore
///
/// #[embassy_executor::task]
/// async fn ble_task(mut runner: Runner<'static, SoftdeviceController<'static>>) {
///     runner.run().await;
/// }
///
/// spawner.must_spawn(ble_task(runner));
/// ```
async fn ble_task<C: Controller, P: PacketPool>(mut runner: Runner<'_, C, P>) {
    loop {
        if let Err(e) = runner.run().await {
            let e = defmt::Debug2Format(&e);
            panic!("[ble_task] error: {:?}", e);
        }
    }
}

/// Stream Events until the connection closes.
///
/// This function will handle the GATT events and process them.
/// This is how we interact with read and write requests.
async fn gatt_events_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) -> Result<(), Error> {
    let pin_data_output = server.pin_service.pin_data_output;
    let pin_data_input = server.pin_service.pin_data_input;
    let adc_data_output = server.pin_service.adc_data_output;
    let reason = loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => break reason,
            GattConnectionEvent::Gatt { event } => {
                info!("[gatt] Gatt Event");
                match &event {
                    GattEvent::Read(event) => {
                        info!("[gatt] Read Event");
                        info!("[gatt] Read Event handle: {:?}", event.handle());
                        info!(
                            "[gatt] pin_data_output handle: {:?}",
                            pin_data_output.handle
                        );
                        if event.handle() == pin_data_output.handle {
                            let value = server.get(&pin_data_output)?;
                            let value_bytes: &[u8] = value.as_ref();
                            match core::str::from_utf8(value_bytes) {
                                Ok(s) => info!(
                                    "[gatt] Read Event to Pin Data Output Characteristic (as string): {}",
                                    s
                                ),
                                Err(_) => info!(
                                    "[gatt] Read Event to Pin Data Output Characteristic (non-UTF8 bytes): {:?}",
                                    defmt::Debug2Format(&value_bytes)
                                ),
                            }
                        } else if event.handle() == adc_data_output.handle {
                            let _value = server.get(&adc_data_output)?;
                            info!("[gatt] Read Event to ADC Data Output Characteristic");
                        }
                    }
                    GattEvent::Write(event) => {
                        info!("[gatt] Write Event");
                        info!("[gatt] Write Event handle: {:?}", event.handle());
                        info!("[gatt] pin_data_input handle: {:?}", pin_data_input.handle);
                        info!(
                            "[gatt] pin_data_output handle: {:?}",
                            pin_data_output.handle
                        );
                        info!("[gatt] Write Event data: {:?}", event.data());
                        let value = event.data();
                        let value_bytes: &[u8] = value.as_ref();
                        if let Ok(str_value) = core::str::from_utf8(value_bytes) {
                            info!("[gatt] Write Event data as string: {}", str_value);
                            let Ok((pin_request, _len)) =
                                serde_json_core::from_str::<PinRequest>(str_value)
                            else {
                                warn!("[gatt] Failed to parse JSON: {}", str_value);
                                continue;
                            };

                            info!("Writing pins");
                            pin_request.pin_writes.iter().for_each(|pin_write| {
                                info!("Writing pin {:?}", pin_write.pin_num);
                                info!("Writing pin state {:?}", pin_write.state);
                                match pin_write.pin_num {
                                    14 => crate::pin::GPIO14_STATE
                                        .store(pin_write.state as u32, Ordering::Relaxed),
                                    26 => crate::pin::GPIO26_STATE
                                        .store(pin_write.state as u32, Ordering::Relaxed),
                                    25 => crate::pin::GPIO25_STATE
                                        .store(pin_write.state as u32, Ordering::Relaxed),
                                    33 => crate::pin::GPIO33_STATE
                                        .store(pin_write.state as u32, Ordering::Relaxed),
                                    _ => {}
                                }
                            });
                        } else {
                            panic!("[gatt] Write Event data is not UTF-8");
                        }
                    }
                    _ => {
                        info!("[gatt] Unknown Event");
                    }
                };
                // This step is also performed at drop(), but writing it explicitly is necessary
                // in order to ensure reply is sent.
                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => warn!(
                        "[gatt] error sending response: {:?}",
                        defmt::Debug2Format(&e)
                    ),
                };
            }
            _ => {} // ignore other Gatt Connection Events
        }
    };
    info!("[gatt] disconnected: {:?}", reason);
    Ok(())
}

/// Create an advertiser to use to connect to a BLE Central, and wait for it to connect.
async fn advertise<'values, 'server, C: Controller>(
    name: &'values str,
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
    server: &'server Server<'values>,
) -> Result<GattConnection<'values, 'server, DefaultPacketPool>, BleHostError<C::Error>> {
    let mut advertiser_data = [0; 31];
    let len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::CompleteLocalName(name.as_bytes()),
        ],
        &mut advertiser_data[..],
    )?;
    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &advertiser_data[..len],
                scan_data: &[],
            },
        )
        .await?;
    info!("[adv] advertising");
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    info!("[adv] connection established");
    Ok(conn)
}

/// Example task to use the BLE notifier interface.
/// This task will notify the connected central of a counter value every 2 seconds.
/// It will also read the RSSI value every 2 seconds.
/// and will stop when the connection is closed by the central or an error occurs.
async fn custom_task<C: Controller, P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    stack: &Stack<'_, C, P>,
    adc_read_pin_nums: Vec<u8>,
) {
    let pin_data_output = server.pin_service.pin_data_output;
    loop {
        let mut data = [0u8; 32];
        let demo_data: &[u8] = &[3u8, 14, 100, 26, 100, 25, 100];
        info!("[custom_task] demo_data length: {:?}", demo_data.len());
        data[..demo_data.len()].copy_from_slice(demo_data);
        if pin_data_output.notify(conn, &data).await.is_ok() {
            info!("[custom_task] Notified connected central of pin data output");
        }

        let adc_data_output = server.pin_service.adc_data_output;
        let num_pins = adc_read_pin_nums.len();
        let mut demo_adc_data: Vec<u8> = Vec::with_capacity(3 * num_pins + 1);
        demo_adc_data.push(num_pins as u8);
        for pin_num in adc_read_pin_nums.clone() {
            let value = match pin_num {
                35 => crate::pin::GPIO35_STATE.load(Ordering::Relaxed),
                32 => crate::pin::GPIO32_STATE.load(Ordering::Relaxed),
                _ => 0,
            };
            let (high, low) = u32_to_u8_pair(value);
            demo_adc_data.push(pin_num as u8);
            demo_adc_data.push(high);
            demo_adc_data.push(low);
        }
        data[..demo_adc_data.len()].copy_from_slice(demo_adc_data.as_slice());
        if adc_data_output.notify(conn, &data).await.is_ok() {
            info!("[custom_task] Notified connected central of adc data output");
        }

        // read RSSI (Received Signal Strength Indicator) of the connection.
        if let Ok(rssi) = conn.raw().rssi(stack).await {
            info!("[custom_task] RSSI: {:?}", rssi);
        } else {
            info!("[custom_task] error getting RSSI");
            break;
        };
        Timer::after_secs(2).await;
    }
}

fn u32_to_u8_pair(value: u32) -> (u8, u8) {
    let high = (value >> 8) as u8;
    let low = value as u8;
    (high, low)
}

// BT format regular num_pins,pin,value,pin,value,... (max 15 pins for now)
// BT format adc num_pins,pin,HighsigByte,LowsigByte,pin,... (max 10 pins for now)

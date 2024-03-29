use embassy_time::{Duration, Timer};
#[cfg(not(feature = "lighting"))]
use futures::future::pending;
use futures::{pin_mut, FutureExt};
use nrf_softdevice::ble::gatt_client::WriteError;
use nrf_softdevice::RawError;

use super::event::OtherEventReceiver;
#[cfg(feature = "lighting")]
use crate::ble::LightingServiceClient;
use crate::ble::{EventServiceClient, FlashServiceClient, PowerServiceClient};
use crate::flash::OtherFlashReceiver;
#[cfg(feature = "lighting")]
use crate::led::OtherLightingReceiver;
use crate::power::OtherPowerReceiver;

pub async fn run_clients(
    flash_client: &FlashServiceClient,
    other_flash_operations: &OtherFlashReceiver,
    power_client: &PowerServiceClient,
    other_power_operations: &OtherPowerReceiver,
    #[cfg(feature = "lighting")] lighting_client: &LightingServiceClient,
    #[cfg(feature = "lighting")] other_lighting_operations: &OtherLightingReceiver,
    event_client: &EventServiceClient,
    other_events: &OtherEventReceiver,
) {
    let flash_future = async {
        loop {
            let other_flash_operation = other_flash_operations.recv().await;
            defmt::info!("Received flash operation for client: {:?}", other_flash_operation);

            loop {
                match flash_client.flash_operation_write(&other_flash_operation).await {
                    Ok(..) => break,
                    Err(WriteError::Raw(RawError::Busy)) => {
                        defmt::warn!("Flash operations busy");
                        Timer::after(Duration::from_millis(10)).await;
                    }
                    Err(error) => panic!("Unexpected write error: {:?}", error),
                }
            }
        }
    }
    .fuse();

    let power_future = async {
        loop {
            let other_power = other_power_operations.recv().await;
            defmt::info!("Received powere for client: {:?}", other_power);

            loop {
                match power_client.power_operation_write(&other_power).await {
                    Ok(..) => break,
                    Err(WriteError::Raw(RawError::Busy)) => {
                        defmt::warn!("Events busy");
                        Timer::after(Duration::from_millis(10)).await;
                    }
                    Err(error) => panic!("Unexpected write error: {:?}", error),
                }
            }
        }
    }
    .fuse();

    #[cfg(feature = "lighting")]
    let lighting_future = async {
        loop {
            let other_lighting_operation = other_lighting_operations.recv().await;
            defmt::info!("Received lighting operation for client: {:?}", other_lighting_operation);

            loop {
                match lighting_client.lighting_operation_write(&other_lighting_operation).await {
                    Ok(..) => break,
                    Err(WriteError::Raw(RawError::Busy)) => {
                        defmt::warn!("Lighting operations busy");
                        Timer::after(Duration::from_millis(10)).await;
                    }
                    Err(error) => panic!("Unexpected write error: {:?}", error),
                }
            }
        }
    }
    .fuse();

    #[cfg(not(feature = "lighting"))]
    let lighting_future = pending::<()>().fuse();

    let event_future = async {
        loop {
            let other_event = other_events.recv().await;
            defmt::info!("Received event for client: {:?}", other_event);

            loop {
                match event_client.event_write(&other_event).await {
                    Ok(..) => break,
                    Err(WriteError::Raw(RawError::Busy)) => {
                        defmt::warn!("Events busy");
                        Timer::after(Duration::from_millis(10)).await;
                    }
                    Err(error) => panic!("Unexpected write error: {:?}", error),
                }
            }
        }
    }
    .fuse();

    pin_mut!(flash_future);
    pin_mut!(power_future);
    pin_mut!(lighting_future);
    pin_mut!(event_future);

    // FIX: use result (?)
    futures::select_biased! {
        _ = flash_future => {},
        _ = power_future => {},
        _ = lighting_future => {},
        _ = event_future => {},
    }
}

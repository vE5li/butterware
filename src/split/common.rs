use embassy_time::{Duration, Timer};
use futures::future::select;
use futures::pin_mut;
use nrf_softdevice::ble::gatt_client::WriteError;
use nrf_softdevice::RawError;

use super::event::OtherEventReceiver;
#[cfg(feature = "lighting")]
use crate::ble::LightingServiceClient;
use crate::ble::{EventServiceClient, FlashServiceClient};
use crate::flash::OtherFlashReceiver;
#[cfg(feature = "lighting")]
use crate::led::OtherLightingReceiver;

pub async fn run_clients(
    flash_client: &FlashServiceClient,
    other_flash_operations: &OtherFlashReceiver,
    lighting_client: &LightingServiceClient,
    other_lighting_operations: &OtherLightingReceiver,
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
                        defmt::error!("flash operations busy");
                        Timer::after(Duration::from_millis(1)).await;
                    }
                    Err(error) => panic!("unexpected write error: {:?}", error),
                }
            }
        }
    };

    let lighting_future = async {
        loop {
            let other_lighting_operation = other_lighting_operations.recv().await;
            defmt::info!("Received lighting operation for client: {:?}", other_lighting_operation);

            loop {
                match lighting_client.lighting_operation_write(&other_lighting_operation).await {
                    Ok(..) => break,
                    Err(WriteError::Raw(RawError::Busy)) => {
                        defmt::error!("lighting operations busy");
                        Timer::after(Duration::from_millis(1)).await;
                    }
                    Err(error) => panic!("unexpected write error: {:?}", error),
                }
            }
        }
    };

    let event_future = async {
        loop {
            let other_event = other_events.recv().await;
            defmt::info!("Received event for client: {:?}", other_event);

            loop {
                match event_client.event_write(&other_event).await {
                    Ok(..) => break,
                    Err(WriteError::Raw(RawError::Busy)) => {
                        defmt::error!("events busy");
                        Timer::after(Duration::from_millis(1)).await;
                    }
                    Err(error) => panic!("unexpected write error: {:?}", error),
                }
            }
        }
    };

    pin_mut!(flash_future);
    pin_mut!(lighting_future);

    // FIX: match ?
    let _ = select(flash_future, lighting_future).await;
}

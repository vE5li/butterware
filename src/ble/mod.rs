use nrf_softdevice::ble::gatt_server::{self, RegisterError, WriteOp};
use nrf_softdevice::ble::{Connection, DeferredReadReply};
use nrf_softdevice::Softdevice;

use crate::interface::{Keyboard, Scannable};

mod advertising;
mod bonder;
mod services;
pub use self::advertising::{AdvertisingData, KEYBOARD_ICON};
pub use self::bonder::Bonder;
use self::services::*;

// Make this a builder struct so we can make the softdevice reference nice
pub struct Server<'a> {
    battery_service: BatteryService,
    device_information_service: DeviceInformationService,
    hid_service: HidService,
    softdevice: Option<&'a Softdevice>,
}

impl<'a> Server<'a> {
    pub fn new(sd: &mut Softdevice) -> Result<Self, RegisterError> {
        let battery_service = BatteryService::new(sd)?;
        let device_information_service = DeviceInformationService::new(sd)?;
        let hid_service = HidService::new(sd)?;

        Ok(Self {
            battery_service,
            device_information_service,
            hid_service,
            softdevice: None,
        })
    }

    pub fn set_softdevice(&mut self, softdevice: &'a Softdevice) {
        self.softdevice = Some(softdevice);
    }

    pub fn send_input_report<K>(&self, connection: &Connection, active_layer: usize, key_state: u64)
    where
        K: Keyboard,
        [(); <K as Scannable>::NAME_LENGTH]:,
        [(); <K as Scannable>::MAXIMUM_ACTIVE_LAYERS]:,
        [(); <K as Scannable>::COLUMNS * <K as Scannable>::ROWS * 2]:,
    {
        self.hid_service.send_input_report::<K>(connection, active_layer, key_state);
    }
}

impl<'a> gatt_server::Server for Server<'a> {
    type Event = ();

    fn on_write(&self, _conn: &Connection, handle: u16, _op: WriteOp, _offset: usize, data: &[u8]) -> Option<Self::Event> {
        defmt::info!("Write");

        self.battery_service.on_write(handle, data);
        self.device_information_service.on_write(handle, data);
        self.hid_service.on_write(handle, data);

        None
    }

    fn on_deferred_read(&self, handle: u16, offset: usize, reply: DeferredReadReply) -> Option<Self::Event> {
        defmt::info!("Deferred read");
        //info!("handle: {}", handle);
        //info!("offset: {}", offset);

        //self.battery_service.on_deferred_read(handle, offset, reply);
        self.hid_service
            .on_deferred_read(self.softdevice.as_ref().unwrap(), handle, offset, reply)
            .unwrap();

        //info!("reply: {:?}", reply);
        None
    }
}

#[nrf_softdevice::gatt_client(uuid = "5a7ef8bc-de9e-11ed-b5ea-0242ac120002")]
pub struct MasterServiceClient {
    #[characteristic(uuid = "66762370-de9e-11ed-b5ea-0242ac120002", read, write)]
    pub other_random_number: u32,
    #[characteristic(uuid = "734e5e64-de9e-11ed-b5ea-0242ac120002", read)]
    pub is_master: bool,
}

#[nrf_softdevice::gatt_service(uuid = "5a7ef8bc-de9e-11ed-b5ea-0242ac120002")]
pub struct MasterService {
    #[characteristic(uuid = "66762370-de9e-11ed-b5ea-0242ac120002", read, write)]
    pub other_random_number: u32,
    #[characteristic(uuid = "734e5e64-de9e-11ed-b5ea-0242ac120002", read)]
    pub is_master: bool,
}

#[nrf_softdevice::gatt_server]
pub struct MasterServer {
    pub master_service: MasterService,
}

#[nrf_softdevice::gatt_service(uuid = "c78c4d70-e02d-11ed-b5ea-0242ac120002")]
pub struct KeyStateService {
    #[characteristic(uuid = "d8004dfa-e02d-11ed-b5ea-0242ac120002", write)]
    pub key_state: u64,
}

#[nrf_softdevice::gatt_client(uuid = "c78c4d70-e02d-11ed-b5ea-0242ac120002")]
pub struct KeyStateServiceClient {
    #[characteristic(uuid = "d8004dfa-e02d-11ed-b5ea-0242ac120002", write)]
    pub key_state: u64,
}

#[nrf_softdevice::gatt_service(uuid = "fe027f36-e7e0-11ed-a05b-0242ac120003")]
pub struct FlashService {
    #[characteristic(uuid = "0b257fe2-e7e1-11ed-a05b-0242ac120003", write)]
    pub flash_operation: crate::flash::FlashOperation,
}

#[nrf_softdevice::gatt_client(uuid = "fe027f36-e7e0-11ed-a05b-0242ac120003")]
pub struct FlashServiceClient {
    #[characteristic(uuid = "0b257fe2-e7e1-11ed-a05b-0242ac120003", write)]
    pub flash_operation: crate::flash::FlashOperation,
}

#[nrf_softdevice::gatt_server]
pub struct KeyStateServer {
    pub key_state_service: KeyStateService,
}

#[nrf_softdevice::gatt_server]
pub struct FlashServer {
    pub flash_service: FlashService,
}


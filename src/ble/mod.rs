use nrf_softdevice::ble::gatt_server::{self, RegisterError, WriteOp};
use nrf_softdevice::ble::{Connection, DeferredReadReply};
use nrf_softdevice::Softdevice;

use crate::{Keyboard, Scannable};

mod bonder;
mod services;
use self::services::*;

pub use self::bonder::Bonder;

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

    pub fn send_input_report<T>(&self, connection: &Connection, key_state: u64)
    where
        T: Keyboard,
        [(); <T as Scannable>::NAME_LENGTH]:,
        [(); <T as Scannable>::COLUMNS * <T as Scannable>::ROWS]:,
    {
        self.hid_service.send_input_report::<T>(connection, key_state);
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

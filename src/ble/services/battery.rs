use nrf_softdevice::ble::gatt_server::builder::ServiceBuilder;
use nrf_softdevice::ble::gatt_server::characteristic::{Attribute, Metadata, Properties};
use nrf_softdevice::ble::gatt_server::{self, RegisterError};
use nrf_softdevice::ble::{Connection, SecurityMode, Uuid};
use nrf_softdevice::Softdevice;

// TODO: Move to own service struct
const BATTERY_LEVEL: Uuid = Uuid::new_16(0x2a19);

// https://www.bluetooth.com/specifications/specs/battery-service/
pub struct BatteryService {
    value_handle: u16,
    cccd_handle: u16,
}

impl BatteryService {
    const UUID: Uuid = Uuid::new_16(0x180f);

    pub fn new(sd: &mut Softdevice) -> Result<Self, RegisterError> {
        let mut service_builder = ServiceBuilder::new(sd, Self::UUID)?;

        let characteristic_builder = service_builder.add_characteristic(
            BATTERY_LEVEL,
            Attribute::new(&[0u8]).security(SecurityMode::JustWorks),
            Metadata::new(Properties::new().read().notify()),
        )?;
        let characteristic_handles = characteristic_builder.build();

        let _service_handle = service_builder.build();

        Ok(Self {
            value_handle: characteristic_handles.value_handle,
            cccd_handle: characteristic_handles.cccd_handle,
        })
    }

    pub fn battery_level_get(&self, sd: &Softdevice) -> Result<u8, gatt_server::GetValueError> {
        let buf = &mut [0u8];
        gatt_server::get_value(sd, self.value_handle, buf)?;
        Ok(buf[0])
    }

    pub fn battery_level_set(&self, sd: &Softdevice, val: u8) -> Result<(), gatt_server::SetValueError> {
        gatt_server::set_value(sd, self.value_handle, &[val])
    }

    pub fn battery_level_notify(&self, conn: &Connection, val: u8) -> Result<(), gatt_server::NotifyValueError> {
        gatt_server::notify_value(conn, self.value_handle, &[val])
    }

    pub fn on_write(&self, handle: u16, data: &[u8]) {
        if handle == self.cccd_handle && !data.is_empty() {
            defmt::info!("battery notifications: {}", (data[0] & 0x01) != 0);
        }
    }
}

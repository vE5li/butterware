use nrf_softdevice::ble::gatt_server::builder::ServiceBuilder;
use nrf_softdevice::ble::gatt_server::characteristic::{Attribute, Metadata, Properties};
use nrf_softdevice::ble::gatt_server::RegisterError;
use nrf_softdevice::ble::{SecurityMode, Uuid};
use nrf_softdevice::Softdevice;

// https://www.bluetooth.com/specifications/specs/device-information-service-1-1/
pub struct DeviceInformationService {
    //value_handle: u16,
    //cccd_handle: u16,
}

impl DeviceInformationService {
    const UUID: Uuid = Uuid::new_16(0x180A);

    pub fn new(sd: &mut Softdevice) -> Result<Self, RegisterError> {
        let mut service_builder = ServiceBuilder::new(sd, Self::UUID)?;

        let attr = Attribute::new(&[0u8]).security(SecurityMode::JustWorks);
        let metadata = Metadata::new(Properties::new().read()); // All characteristics for the
        // device information service are
        // read only
        //let characteristic_builder =
        // service_builder.add_characteristic(DEVICE_INFORMATION, attr, metadata)?;
        // let characteristic_handles = characteristic_builder.build();

        let _service_handle = service_builder.build();

        Ok(Self {
            //value_handle: characteristic_handles.value_handle,
            //cccd_handle: characteristic_handles.cccd_handle,
        })
    }

    pub fn on_write(&self, handle: u16, data: &[u8]) {
        //if handle == self.cccd_handle && !data.is_empty() {
        //    info!("device information notifications: {}", (data[0] & 0x01) !=
        // 0);
        //}
    }
}

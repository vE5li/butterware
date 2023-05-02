use core::marker::PhantomData;

use nrf_softdevice::ble::gatt_server::builder::ServiceBuilder;
use nrf_softdevice::ble::gatt_server::characteristic::{Attribute, Metadata, Properties};
use nrf_softdevice::ble::gatt_server::{self, CharacteristicHandles, DescriptorHandle, RegisterError};
use nrf_softdevice::ble::{Connection, DeferredReadReply, SecurityMode, Uuid};
use nrf_softdevice::Softdevice;

use crate::interface::{Keyboard, Scannable};
use crate::hardware::TestBit;

pub struct InputReport;
pub struct OutpetReport;
pub struct FeatureReport;

pub trait ReportCharacteristicType {
    const MAX_LENGTH: u16;
    const REFERENCE_ID: u8;
    // https://infocenter.nordicsemi.com/index.jsp?topic=%2Fcom.nordic.infocenter.sdk5.v13.0.0%2Fgroup__ble__hids.html
    const REFERENCE_TYPE: u8;
    const PROPERTIES: Properties;
    const NAME: &'static str;
}

impl ReportCharacteristicType for InputReport {
    const MAX_LENGTH: u16 = 8;
    const NAME: &'static str = "Input";
    const PROPERTIES: Properties = Properties::new().read().write().notify();
    const REFERENCE_ID: u8 = 0;
    const REFERENCE_TYPE: u8 = 1;
}

impl ReportCharacteristicType for OutpetReport {
    const MAX_LENGTH: u16 = 1;
    const NAME: &'static str = "Output";
    const PROPERTIES: Properties = Properties::new().read().write().write_without_response();
    // FIX: probably also 8 or something
    const REFERENCE_ID: u8 = 0;
    const REFERENCE_TYPE: u8 = 2;
}

impl ReportCharacteristicType for FeatureReport {
    const MAX_LENGTH: u16 = 1;
    const NAME: &'static str = "Feature";
    const PROPERTIES: Properties = Properties::new().read().write();
    const REFERENCE_ID: u8 = 0;
    const REFERENCE_TYPE: u8 = 3;
}

pub struct ReportCharacteristic<T: ReportCharacteristicType> {
    characteristic_handles: CharacteristicHandles,
    descriptor_handle: DescriptorHandle,
    phantom_data: PhantomData<T>,
}

impl<T: ReportCharacteristicType> ReportCharacteristic<T> {
    // TODO: make these const generics (?)
    pub fn new(service_builder: &mut ServiceBuilder<'_>) -> Result<Self, RegisterError> {
        // TODO: move somewhere else, maybe associated constants
        const REPORT: Uuid = Uuid::new_16(0x2A4D);
        const REPORT_REF_DESCRIPTOR: Uuid = Uuid::new_16(0x2908);

        let mut characteristic_builder = service_builder.add_characteristic(
            REPORT,
            Attribute::new(&[])
                .security(SecurityMode::JustWorks)
                .variable_len(T::MAX_LENGTH)
                .deferred_read(),
            Metadata::new(T::PROPERTIES), // write only if security mode
        )?;

        // Reference descriptor
        let mut buffer = [0; 2];
        Self::report_ref_encode(&mut buffer, T::REFERENCE_ID, T::REFERENCE_TYPE);
        let max_length = buffer.len() as u16;

        // TODO: double check that Max length == Initial length

        let descriptor_handle = characteristic_builder
            .add_descriptor(
                REPORT_REF_DESCRIPTOR,
                Attribute::new(buffer)
                    .security(SecurityMode::JustWorks)
                    .variable_len(max_length)
                    .deferred_read(),
            )
            .unwrap();

        let characteristic_handles = characteristic_builder.build();

        Ok(Self {
            characteristic_handles,
            descriptor_handle,
            phantom_data: PhantomData::default(),
        })
    }

    pub fn on_write(&self, handle: u16, data: &[u8]) {
        if handle == self.characteristic_handles.cccd_handle && !data.is_empty() {
            defmt::info!("{} report notifications: {}", T::NAME, (data[0] & 0x01) != 0);
        }
    }

    pub fn on_deferred_read(&self, softdevice: &Softdevice, handle: u16, offset: usize) {
        //defmt::info!("{:?}", self.characteristic_handles);
        //defmt::info!("{:?}", self.descriptor_handle);

        if handle == self.characteristic_handles.value_handle {
            defmt::info!("dererred read on hid {} report characteristic with offset {}", T::NAME, offset);
        }

        if handle == self.descriptor_handle.handle() {
            defmt::info!("dererred read on hid {} report descriptor with offset {}", T::NAME, offset);
        }
    }

    // TODO: remove or rework
    fn report_ref_encode(buffer: &mut [u8; 2], report_id: u8, report_type: u8) -> usize {
        let mut len = 0;

        buffer[len] = report_id;
        len += 1;

        buffer[len] = report_type;
        len += 1;

        return len;
    }
}

pub struct ReportMapCharacteristic {}

impl ReportMapCharacteristic {
    pub fn new(service_builder: &mut ServiceBuilder<'_>) -> Result<Self, RegisterError> {
        const REPORT_MAP: Uuid = Uuid::new_16(0x2A4B);

        const MAP_DATA: &[u8] = &[
            0x05, 0x01, // Usage Page (Generic Desktop)
            0x09, 0x06, // Usage (Keyboard)
            0xA1, 0x01, // Collection (Application)
            0x05, 0x07, // Usage Page (Key Codes)
            0x19, 0xe0, // Usage Minimum (224)
            0x29, 0xe7, // Usage Maximum (231)
            0x15, 0x00, // Logical Minimum (0)
            0x25, 0x01, // Logical Maximum (1)
            0x75, 0x01, // Report Size (1)
            0x95, 0x08, // Report Count (8)
            0x81, 0x02, // Input (Data, Variable, Absolute)
            0x95, 0x01, // Report Count (1)
            0x75, 0x08, // Report Size (8)
            0x81, 0x01, // Input (Constant) reserved byte(1)
            0x95, 0x05, // Report Count (5)
            0x75, 0x01, // Report Size (1)
            0x05, 0x08, // Usage Page (Page# for LEDs)
            0x19, 0x01, // Usage Minimum (1)
            0x29, 0x05, // Usage Maximum (5)
            0x91, 0x02, // Output (Data, Variable, Absolute), Led report
            0x95, 0x01, // Report Count (1)
            0x75, 0x03, // Report Size (3)
            0x91, 0x01, // Output (Data, Variable, Absolute), Led report padding
            0x95, 0x06, // Report Count (6)
            0x75, 0x08, // Report Size (8)
            0x15, 0x00, // Logical Minimum (0)
            0x25, 0x65, // Logical Maximum (101)
            0x05, 0x07, // Usage Page (Key codes)
            0x19, 0x00, // Usage Minimum (0)
            0x29, 0x65, // Usage Maximum (101)
            0x81, 0x00, // Input (Data, Array) Key array(6 bytes)
            0x09, 0x05, // Usage (Vendor Defined)
            0x15, 0x00, // Logical Minimum (0)
            0x26, 0xFF, 0x00, // Logical Maximum (255)
            0x75, 0x08, // Report Size (8 bit)
            0x95, 0x02, // Report Count (2)
            0xB1, 0x02, // Feature (Data, Variable, Absolute)
            0xC0, // End Collection (Application)
        ];

        let characteristic_builder = service_builder.add_characteristic(
            REPORT_MAP,
            Attribute::new(MAP_DATA)
                .variable_len(MAP_DATA.len() as u16)
                .security(SecurityMode::JustWorks),
            Metadata::new(Properties::new().read()),
        )?;
        let _characteristic_handles = characteristic_builder.build();

        Ok(Self {})
    }
}

pub struct BootInputReportKeyboard;
pub struct BootOutputReportKeyboard;
pub struct BootInputReportMouse;

pub trait BootReportDevice {
    const UUID: Uuid;
    const MAX_LENGTH: usize;
    const PROPERTIES: Properties;
    const NAME: &'static str;
}

impl BootReportDevice for BootInputReportKeyboard {
    const MAX_LENGTH: usize = 8;
    const NAME: &'static str = "input keyboard";
    const PROPERTIES: Properties = Properties::new().read().write().notify();
    const UUID: Uuid = Uuid::new_16(0x2A22);
}

impl BootReportDevice for BootOutputReportKeyboard {
    const MAX_LENGTH: usize = 1;
    const NAME: &'static str = "output keyboard";
    const PROPERTIES: Properties = Properties::new().read().write().write_without_response();
    const UUID: Uuid = Uuid::new_16(0x2A32);
}

impl BootReportDevice for BootInputReportMouse {
    const MAX_LENGTH: usize = 8;
    const NAME: &'static str = "input mouse";
    const PROPERTIES: Properties = Properties::new().read().write().notify();
    const UUID: Uuid = Uuid::new_16(0x2A33);
}

pub struct BootReportCharacteristic<T: BootReportDevice> {
    characteristic_handles: CharacteristicHandles,
    phantom_data: PhantomData<T>,
}

impl<T: BootReportDevice> BootReportCharacteristic<T> {
    pub fn new(service_builder: &mut ServiceBuilder<'_>) -> Result<Self, RegisterError>
    where
        [(); T::MAX_LENGTH]:,
    {
        let characteristic_builder = service_builder.add_characteristic(
            T::UUID,
            Attribute::new(&[0; T::MAX_LENGTH])
                // TODO: confirm that this security level is actually fine since it's different in the
                // reference
                .security(SecurityMode::JustWorks)
                .deferred_read(),
            Metadata::new(Properties::new().read().write().notify()),
        )?;
        let characteristic_handles = characteristic_builder.build();

        Ok(Self {
            characteristic_handles,
            phantom_data: PhantomData::default(),
        })
    }

    pub fn on_write(&self, handle: u16, data: &[u8]) {
        if handle == self.characteristic_handles.cccd_handle && !data.is_empty() {
            defmt::info!("{} boot report notifications: {}", T::NAME, (data[0] & 0x01) != 0);
        }
    }

    pub fn on_deferred_read(&self, softdevice: &Softdevice, handle: u16, offset: usize) {
        //defmt::info!("{:?}", self.characteristic_handles);
        //defmt::info!("{:?}", self.descriptor_handle);

        if handle == self.characteristic_handles.value_handle {
            defmt::info!(
                "dererred read on hid boot {} report characteristic with offset {}",
                T::NAME,
                offset
            );
        }
    }
}

pub struct HidInformationCharacteristic {}

impl HidInformationCharacteristic {
    pub fn new(service_builder: &mut ServiceBuilder<'_>) -> Result<Self, RegisterError> {
        const UUID: Uuid = Uuid::new_16(0x2A4A);

        // 0x00 for unlocalized
        // TODO: (double check)
        const COUNTRY_CODE: u8 = 0;

        const HID_INFO_FLAG_REMOTE_WAKE_MSK: u8 = 0x1;
        const HID_INFO_FLAG_NORMALLY_CONNECTABLE_MSK: u8 = 0x2;
        const FLAGS: u8 = HID_INFO_FLAG_REMOTE_WAKE_MSK | HID_INFO_FLAG_NORMALLY_CONNECTABLE_MSK;

        const USB_HID_SPEC_VERSION: u16 = 0x0101;

        let encoded_data = &[USB_HID_SPEC_VERSION as u8, (USB_HID_SPEC_VERSION >> 8) as u8, COUNTRY_CODE, FLAGS];

        let characteristic_builder = service_builder.add_characteristic(
            UUID,
            Attribute::new(encoded_data).security(SecurityMode::JustWorks),
            Metadata::new(Properties::new().read()),
        )?;
        let _characteristic_handles = characteristic_builder.build();

        Ok(Self {})
    }
}

pub struct ControlPointCharacteristic {}

impl ControlPointCharacteristic {
    pub fn new(service_builder: &mut ServiceBuilder<'_>) -> Result<Self, RegisterError> {
        const UUID: Uuid = Uuid::new_16(0x2A4C);
        const INITIAL_VALUE: &[u8] = &[0];

        let characteristic_builder = service_builder.add_characteristic(
            UUID,
            Attribute::new(INITIAL_VALUE).security(SecurityMode::JustWorks),
            Metadata::new(Properties::new().write_without_response()),
        )?;
        let _characteristic_handles = characteristic_builder.build();

        Ok(Self {})
    }
}

// https://www.bluetooth.com/specifications/specs/human-interface-device-service-1-0/
pub struct HidService {
    input_report: ReportCharacteristic<InputReport>,
    output_report: ReportCharacteristic<OutpetReport>,
    feature_report: ReportCharacteristic<FeatureReport>,
    report_map: ReportMapCharacteristic,
    boot_input_report: BootReportCharacteristic<BootInputReportKeyboard>,
    boot_output_report: BootReportCharacteristic<BootOutputReportKeyboard>,
    hid_information: HidInformationCharacteristic,
    control_point: ControlPointCharacteristic,
}

impl HidService {
    const UUID: Uuid = Uuid::new_16(0x1812);

    pub fn new(sd: &mut Softdevice) -> Result<Self, RegisterError> {
        const PROTOCOL_MODE: Uuid = Uuid::new_16(0x2A4E);

        #[allow(unused)]
        const PROTOCOL_MODE_BOOT: u8 = 0x0;
        const PROTOCOL_MODE_REPORT: u8 = 0x1;

        let mut service_builder = ServiceBuilder::new(sd, Self::UUID)?;

        // Protocol mode characteristic
        let characteristic_builder = service_builder.add_characteristic(
            PROTOCOL_MODE,
            Attribute::new(&[PROTOCOL_MODE_REPORT]).security(SecurityMode::JustWorks),
            Metadata::new(Properties::new().read().write_without_response()),
        )?;
        let _characteristic_handles = characteristic_builder.build();

        // Input-, output-, and feature-characteristic
        let input_report = ReportCharacteristic::new(&mut service_builder)?;
        let output_report = ReportCharacteristic::new(&mut service_builder)?;
        let feature_report = ReportCharacteristic::new(&mut service_builder)?;

        // Report map characteristic
        let report_map = ReportMapCharacteristic::new(&mut service_builder)?;

        // Boot keyboard characteristic
        let boot_input_report = BootReportCharacteristic::new(&mut service_builder)?;
        let boot_output_report = BootReportCharacteristic::new(&mut service_builder)?;

        // HID information characteristic
        let hid_information = HidInformationCharacteristic::new(&mut service_builder)?;

        // Control point characteristic
        let control_point = ControlPointCharacteristic::new(&mut service_builder)?;

        let _service_handle = service_builder.build();

        Ok(Self {
            input_report,
            output_report,
            feature_report,
            report_map,
            boot_input_report,
            boot_output_report,
            hid_information,
            control_point,
        })
    }

    pub fn on_write(&self, handle: u16, data: &[u8]) {
        self.input_report.on_write(handle, data);
        self.output_report.on_write(handle, data);
        self.feature_report.on_write(handle, data);

        self.boot_input_report.on_write(handle, data);
        self.boot_output_report.on_write(handle, data);
    }

    pub fn send_input_report<K>(&self, connection: &Connection, active_layer: usize, key_state: u64)
    where
        K: Keyboard,
        [(); <K as Scannable>::MAXIMUM_ACTIVE_LAYERS]:,
        [(); <K as Scannable>::COLUMNS * <K as Scannable>::ROWS * 2]:,
    {
        const SCAN_CODE_POSITION: usize = 2;
        const REPORT_SIZE: usize = 8;

        let mut input_report = [0; REPORT_SIZE];
        let mut offset = SCAN_CODE_POSITION;

        // temporary assert to avoid bugs while implementing.
        assert!(<K as Scannable>::COLUMNS * <K as Scannable>::ROWS * 2 <= 64);

        for index in 0..<K as Scannable>::COLUMNS * <K as Scannable>::ROWS * 2 {
            if key_state.test_bit(index) {
                if offset == REPORT_SIZE {
                    input_report[SCAN_CODE_POSITION..REPORT_SIZE].fill(crate::keys::ERR_OVF.keycode());
                    break;
                }

                let key = K::LAYER_LOOKUP[active_layer][index].keycode();
                input_report[offset] = key;
                offset += 1;
            }
        }

        defmt::info!("Sending input report with value {:?}", input_report);

        gatt_server::notify_value(connection, self.input_report.characteristic_handles.value_handle, &input_report).unwrap();
    }

    pub fn on_deferred_read(
        &self,
        softdevice: &Softdevice,
        handle: u16,
        offset: usize,
        reply: DeferredReadReply,
    ) -> Result<bool, gatt_server::GetValueError> {
        //info!("{:?}", self.input_report_characteristic_handles);
        //info!("{:?}", self.input_report_descriptor_handle);

        self.input_report.on_deferred_read(softdevice, handle, offset);
        self.output_report.on_deferred_read(softdevice, handle, offset);
        self.feature_report.on_deferred_read(softdevice, handle, offset);

        self.boot_input_report.on_deferred_read(softdevice, handle, offset);
        self.boot_output_report.on_deferred_read(softdevice, handle, offset);

        reply.reply(Ok(None)).unwrap();
        Ok(false)
    }
}

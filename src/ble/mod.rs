mod advertising;
mod bonder;

pub use self::advertising::{AdvertisingData, KEYBOARD_ICON};
pub use self::bonder::Bonder;

#[nrf_softdevice::gatt_service(uuid = "180f")]
pub struct BatteryService {
    #[characteristic(uuid = "2a19", security = "justworks", read, notify)]
    pub battery_level: u8,
}

#[nrf_softdevice::gatt_service(uuid = "180A")]
pub struct DeviceInformationService {}

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

const NO_DATA: &[u8] = &[];
const INPUT_VALUE: [u8; 2] = [0, 1];
const OUTPUT_VALUE: [u8; 2] = [0, 2];
const FEATURE_VALUE: [u8; 2] = [0, 3];
const BOOT_INPUT_REPORT_VALUE: [u8; 8] = [0; 8];
const BOOT_OUTPUT_REPORT_VALUE: [u8; 1] = [0; 1];
const HID_INFORMATION_VALUE: [u8; 4] = [USB_HID_SPEC_VERSION as u8, (USB_HID_SPEC_VERSION >> 8) as u8, COUNTRY_CODE, FLAGS];
const CONTROL_POINT_VALUE: [u8; 1] = [0; 1];

const COUNTRY_CODE: u8 = 0;
const HID_INFO_FLAG_REMOTE_WAKE_MSK: u8 = 0x1;
const HID_INFO_FLAG_NORMALLY_CONNECTABLE_MSK: u8 = 0x2;
const FLAGS: u8 = HID_INFO_FLAG_REMOTE_WAKE_MSK | HID_INFO_FLAG_NORMALLY_CONNECTABLE_MSK;
const USB_HID_SPEC_VERSION: u16 = 0x0101;

#[nrf_softdevice::gatt_service(uuid = "1812")]
pub struct HidService {
    #[characteristic(
        uuid = "2A4D",
        initial_value = "NO_DATA",
        security = "justworks",
        read,
        write,
        notify,
        descriptor(uuid = "2908", security = "justworks", value = "INPUT_VALUE")
    )]
    pub input_report: [u8; 8],
    #[characteristic(
        uuid = "2A4D",
        initial_value = "NO_DATA",
        security = "justworks",
        read,
        write,
        write_without_response,
        descriptor(uuid = "2908", security = "justworks", value = "OUTPUT_VALUE")
    )]
    pub output_report: [u8; 1],
    #[characteristic(
        uuid = "2A4D",
        initial_value = "NO_DATA",
        security = "justworks",
        read,
        write,
        descriptor(uuid = "2908", security = "justworks", value = "FEATURE_VALUE")
    )]
    pub feature_report: [u8; 1],
    #[characteristic(uuid = "2A4B", initial_value = "MAP_DATA", security = "justworks", read)]
    pub report_map: [u8; MAP_DATA.len()],
    #[characteristic(
        uuid = "2A22",
        initial_value = "BOOT_INPUT_REPORT_VALUE",
        security = "justworks",
        read,
        write,
        notify
    )]
    pub boot_input_report: [u8; 8],
    #[characteristic(
        uuid = "2A32",
        initial_value = "BOOT_OUTPUT_REPORT_VALUE",
        security = "justworks",
        read,
        write,
        write_without_response
    )]
    pub boot_output_report: [u8; 1],
    #[characteristic(uuid = "2A4A", initial_value = "HID_INFORMATION_VALUE", security = "justworks", read)]
    pub hid_information: [u8; 4],
    #[characteristic(
        uuid = "2A4C",
        initial_value = "CONTROL_POINT_VALUE",
        security = "justworks",
        write_without_response
    )]
    pub control_point: u8,
}

#[nrf_softdevice::gatt_server]
pub struct Server {
    pub battery_service: BatteryService,
    pub device_information_service: DeviceInformationService,
    pub hid_service: HidService,
}

#[nrf_softdevice::gatt_service(uuid = "5a7ef8bc-de9e-11ed-b5ea-0242ac120002")]
pub struct MasterService {
    #[characteristic(uuid = "66762370-de9e-11ed-b5ea-0242ac120002", read, write)]
    pub other_random_number: u32,
    #[characteristic(uuid = "734e5e64-de9e-11ed-b5ea-0242ac120002", read)]
    pub is_master: bool,
}

#[nrf_softdevice::gatt_client(uuid = "5a7ef8bc-de9e-11ed-b5ea-0242ac120002")]
pub struct MasterServiceClient {
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

#[cfg(feature = "lighting")]
#[nrf_softdevice::gatt_service(uuid = "01c5abb0-ee81-11ed-a05b-0242ac120003")]
pub struct LightingService {
    #[characteristic(uuid = "0bad19c4-ee81-11ed-a05b-0242ac120003", write)]
    pub lighting_operation: crate::led::LightingOperation,
}

#[cfg(feature = "lighting")]
#[nrf_softdevice::gatt_client(uuid = "01c5abb0-ee81-11ed-a05b-0242ac120003")]
pub struct LightingServiceClient {
    #[characteristic(uuid = "0bad19c4-ee81-11ed-a05b-0242ac120003", write)]
    pub lighting_operation: crate::led::LightingOperation,
}

#[nrf_softdevice::gatt_service(uuid = "465b52c4-f0c6-11ed-a05b-0242ac120003")]
pub struct EventService {
    #[characteristic(uuid = "579bce56-f0c6-11ed-a05b-0242ac120003", write)]
    pub event: crate::split::UsedEvent,
}

#[cfg(feature = "lighting")]
#[nrf_softdevice::gatt_client(uuid = "465b52c4-f0c6-11ed-a05b-0242ac120003")]
pub struct EventServiceClient {
    #[characteristic(uuid = "579bce56-f0c6-11ed-a05b-0242ac120003", write)]
    pub event: crate::split::UsedEvent,
}

#[nrf_softdevice::gatt_server]
pub struct CommunicationServer {
    pub key_state_service: KeyStateService,
    pub flash_service: FlashService,
    #[cfg(feature = "lighting")]
    pub lighting_service: LightingService,
    pub event_service: EventService,
}

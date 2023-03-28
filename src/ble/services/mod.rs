// List of characteristic with their respective UUIDs
// https://github.com/sputnikdev/bluetooth-gatt-parser/tree/master/src/main/resources/gatt/characteristic

mod battery;
mod device_information;
mod hid;

pub use self::battery::BatteryService;
pub use self::device_information::DeviceInformationService;
pub use self::hid::HidService;

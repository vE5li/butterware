#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr)]
#![feature(concat_idents)]
#![feature(iter_next_chunk)]

use defmt_rtt as _; // global logger
use embassy_nrf as _; // time driver
use panic_probe as _;

use core::cell::{Cell, RefCell};
use core::marker::PhantomData;
use core::mem;

use defmt::{info, *};
use embassy_executor::Spawner;
use embassy_nrf::config::{HfclkSource, LfclkSource};
use embassy_nrf::gpio::{AnyPin, Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::peripherals::SPI3;
use embassy_nrf::spim::{Config, Spim};
use embassy_nrf::{interrupt, Peripherals};
use embassy_time::{Duration, Timer};
use futures::future::{select, Either};
use futures::pin_mut;
use nrf_softdevice::ble::gatt_server::builder::ServiceBuilder;
use nrf_softdevice::ble::gatt_server::characteristic::{Attribute, Metadata, Properties};
use nrf_softdevice::ble::gatt_server::{
    set_sys_attrs, CharacteristicHandles, DescriptorHandle, RegisterError, WriteOp,
};
use nrf_softdevice::ble::security::{IoCapabilities, SecurityHandler};
use nrf_softdevice::ble::{
    gatt_server, peripheral, Connection, DeferredReadReply, EncryptionInfo, IdentityKey, MasterId, SecurityMode, Uuid,
};
use nrf_softdevice::{raw, Softdevice};
use static_cell::StaticCell;

#[allow(unused)]
mod keys;
use keys::*;

// List of characteristic with their respective UUIDs
// https://github.com/sputnikdev/bluetooth-gatt-parser/tree/master/src/main/resources/gatt/characteristic
const BATTERY_SERVICE: Uuid = Uuid::new_16(0x180f);
const BATTERY_LEVEL: Uuid = Uuid::new_16(0x2a19);

const DEVICE_INFORMATION_SERVICE: Uuid = Uuid::new_16(0x180A);

const HID_SERVICE: Uuid = Uuid::new_16(0x1812);

macro_rules! register_layers {
    ($board:ident, $layers:ident, [$($names:ident),*]) => {
        struct $layers;

        impl $layers {
            $(pub const $names: Layer = Layer(${index()});)*
            pub const LAYER_LOOKUP: &'static [&'static [Mapping; <$board as Scannable>::COLUMNS * <$board as Scannable>::ROWS]] = &[$(&$board::$names),*];
        }
    };
}

#[path ="../keyboards/mod.rs"]
mod keyboards;

use keyboards::Used;

/*macro_rules! layout {
    ($($names:ident,)*) => {{
        &[$(core::concat_idents!(KEY_, $names)),*]
    }};
}*/

/*macro_rules! matrix {
    ($($lookup:expr,)*) => {
        const _MATRIX: &[usize] = {
            &[$($lookup),*]
        };
    };
}*/

trait UnwrapInfelliable {
    type Output;

    fn unwrap_infelliable(self) -> Self::Output;
}

impl<T, E> UnwrapInfelliable for Result<T, E> {
    type Output = T;

    fn unwrap_infelliable(self) -> Self::Output {
        match self {
            Ok(value) => value,
            Err(..) => crate::unreachable!(),
        }
    }
}

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

#[derive(Debug, Clone, Copy)]
struct Peer {
    master_id: MasterId,
    key: EncryptionInfo,
    peer_id: IdentityKey,
}

pub struct Bonder {
    peer: Cell<Option<Peer>>,
    sys_attrs: RefCell<heapless::Vec<u8, 62>>,
}

impl Default for Bonder {
    fn default() -> Self {
        Bonder {
            peer: Cell::new(None),
            sys_attrs: Default::default(),
        }
    }
}

impl SecurityHandler for Bonder {
    fn io_capabilities(&self) -> IoCapabilities {
        IoCapabilities::None
    }

    fn can_bond(&self, _conn: &Connection) -> bool {
        true
    }

    fn display_passkey(&self, passkey: &[u8; 6]) {
        info!("The passkey is \"{:a}\"", passkey)
    }

    fn on_bonded(&self, _conn: &Connection, master_id: MasterId, key: EncryptionInfo, peer_id: IdentityKey) {
        debug!("storing bond for: id: {}, key: {}", master_id, key);

        // In a real application you would want to signal another task to permanently store the keys in non-volatile memory here.
        self.sys_attrs.borrow_mut().clear();
        self.peer.set(Some(Peer {
            master_id,
            key,
            peer_id,
        }));
    }

    fn get_key(&self, _conn: &Connection, master_id: MasterId) -> Option<EncryptionInfo> {
        debug!("getting bond for: id: {}", master_id);

        self.peer
            .get()
            .and_then(|peer| (master_id == peer.master_id).then_some(peer.key))
    }

    fn save_sys_attrs(&self, conn: &Connection) {
        debug!("saving system attributes for: {}", conn.peer_address());

        if let Some(peer) = self.peer.get() {
            if peer.peer_id.is_match(conn.peer_address()) {
                let mut sys_attrs = self.sys_attrs.borrow_mut();
                let capacity = sys_attrs.capacity();
                unwrap!(sys_attrs.resize(capacity, 0));
                let len = unwrap!(gatt_server::get_sys_attrs(conn, &mut sys_attrs)) as u16;
                sys_attrs.truncate(usize::from(len));
                // In a real application you would want to signal another task to permanently store sys_attrs for this connection's peer
            }
        }
    }

    fn load_sys_attrs(&self, conn: &Connection) {
        let addr = conn.peer_address();
        debug!("loading system attributes for: {}", addr);

        let attrs = self.sys_attrs.borrow();
        // In a real application you would search all stored peers to find a match
        let attrs = if self.peer.get().map(|peer| peer.peer_id.is_match(addr)).unwrap_or(false) {
            (!attrs.is_empty()).then_some(attrs.as_slice())
        } else {
            None
        };

        unwrap!(set_sys_attrs(conn, attrs));
    }
}

// https://www.bluetooth.com/specifications/specs/battery-service/
pub struct BatteryService {
    value_handle: u16,
    cccd_handle: u16,
}

impl BatteryService {
    pub fn new(sd: &mut Softdevice) -> Result<Self, RegisterError> {
        let mut service_builder = ServiceBuilder::new(sd, BATTERY_SERVICE)?;

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
            info!("battery notifications: {}", (data[0] & 0x01) != 0);
        }
    }

    pub fn on_deferred_read(&self, handle: u16, offset: usize, reply: DeferredReadReply) -> bool {
        if handle == self.value_handle {
            info!("dererred read on battery with offset {}", offset);
            return true;
        }

        false
    }
}

// https://www.bluetooth.com/specifications/specs/device-information-service-1-1/
pub struct DeviceInformationService {
    //value_handle: u16,
    //cccd_handle: u16,
}

impl DeviceInformationService {
    pub fn new(sd: &mut Softdevice) -> Result<Self, RegisterError> {
        let mut service_builder = ServiceBuilder::new(sd, DEVICE_INFORMATION_SERVICE)?;

        let attr = Attribute::new(&[0u8]).security(SecurityMode::JustWorks);
        let metadata = Metadata::new(Properties::new().read()); // All characteristics for the
                                                                // device information service are
                                                                // read only
                                                                //let characteristic_builder = service_builder.add_characteristic(DEVICE_INFORMATION, attr, metadata)?;
                                                                //let characteristic_handles = characteristic_builder.build();

        let _service_handle = service_builder.build();

        Ok(Self {
            //value_handle: characteristic_handles.value_handle,
            //cccd_handle: characteristic_handles.cccd_handle,
        })
    }

    pub fn on_write(&self, handle: u16, data: &[u8]) {
        //if handle == self.cccd_handle && !data.is_empty() {
        //    info!("device information notifications: {}", (data[0] & 0x01) != 0);
        //}
    }
}

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
    const REFERENCE_ID: u8 = 0;
    const REFERENCE_TYPE: u8 = 1;
    const PROPERTIES: Properties = Properties::new().read().write().notify();
    const NAME: &'static str = "Input";
}

impl ReportCharacteristicType for OutpetReport {
    const MAX_LENGTH: u16 = 1; // FIX: probably also 8 or something
    const REFERENCE_ID: u8 = 0;
    const REFERENCE_TYPE: u8 = 2;
    const PROPERTIES: Properties = Properties::new().read().write().write_without_response();
    const NAME: &'static str = "Output";
}

impl ReportCharacteristicType for FeatureReport {
    const MAX_LENGTH: u16 = 1;
    const REFERENCE_ID: u8 = 0;
    const REFERENCE_TYPE: u8 = 3;
    const PROPERTIES: Properties = Properties::new().read().write();
    const NAME: &'static str = "Feature";
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
            info!("{} report notifications: {}", T::NAME, (data[0] & 0x01) != 0);
        }
    }

    pub fn on_deferred_read(&self, softdevice: &Softdevice, handle: u16, offset: usize) {
        //info!("{:?}", self.characteristic_handles);
        //info!("{:?}", self.descriptor_handle);

        if handle == self.characteristic_handles.value_handle {
            info!(
                "dererred read on hid {} report characteristic with offset {}",
                T::NAME,
                offset
            );
        }

        if handle == self.descriptor_handle.handle() {
            info!(
                "dererred read on hid {} report descriptor with offset {}",
                T::NAME,
                offset
            );
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
    const UUID: Uuid = Uuid::new_16(0x2A22);
    const MAX_LENGTH: usize = 8;
    const PROPERTIES: Properties = Properties::new().read().write().notify();
    const NAME: &'static str = "input keyboard";
}

impl BootReportDevice for BootOutputReportKeyboard {
    const UUID: Uuid = Uuid::new_16(0x2A32);
    const MAX_LENGTH: usize = 1;
    const PROPERTIES: Properties = Properties::new().read().write().write_without_response();
    const NAME: &'static str = "output keyboard";
}

impl BootReportDevice for BootInputReportMouse {
    const UUID: Uuid = Uuid::new_16(0x2A33);
    const MAX_LENGTH: usize = 8;
    const PROPERTIES: Properties = Properties::new().read().write().notify();
    const NAME: &'static str = "input mouse";
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
            info!("{} boot report notifications: {}", T::NAME, (data[0] & 0x01) != 0);
        }
    }

    pub fn on_deferred_read(&self, softdevice: &Softdevice, handle: u16, offset: usize) {
        //info!("{:?}", self.characteristic_handles);
        //info!("{:?}", self.descriptor_handle);

        if handle == self.characteristic_handles.value_handle {
            info!(
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

        let encoded_data = &[
            USB_HID_SPEC_VERSION as u8,
            (USB_HID_SPEC_VERSION >> 8) as u8,
            COUNTRY_CODE,
            FLAGS,
        ];

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

pub trait Scannable {
    const COLUMNS: usize;
    const ROWS: usize;
}

pub trait Keyboard: Scannable
where
    [(); Self::COLUMNS * Self::ROWS]:,
{
    const DEVICE_NAME: &'static str;

    const MATRIX: [usize; Self::COLUMNS * Self::ROWS];

    const LAYER_LOOKUP: &'static [&'static [Mapping; Self::COLUMNS * Self::ROWS]];

    fn new() -> Self;

    fn init_peripherals(&mut self, peripherals: Peripherals) -> ScanPinConfig<{ Self::COLUMNS }, { Self::ROWS }>;
}

pub struct SpiConfig {
    interface: SPI3,
    interrupt: embassy_nrf::interrupt::SPIM3,
    clock_pin: AnyPin,
    mosi_pin: AnyPin,
    config: Config,
}

pub struct ScanPinConfig<const C: usize, const R: usize> {
    columns: [AnyPin; C],
    rows: [AnyPin; R],
    power_pin: Option<AnyPin>,
    spi_config: Option<SpiConfig>,
}

impl<const C: usize, const R: usize> ScanPinConfig<C, R> {
    pub fn to_pins(self) -> ScanPins<'static, C, R> {
        ScanPins {
            columns: self
                .columns
                .into_iter()
                .map(|pin| Output::new(pin, Level::Low, OutputDrive::Standard))
                .next_chunk()
                .unwrap_infelliable(),
            rows: self
                .rows
                .into_iter()
                .map(|pin| Input::new(pin, Pull::Down))
                .next_chunk()
                .unwrap_infelliable(),
            power_pin: self
                .power_pin
                .map(|pin| Output::new(pin, Level::High, OutputDrive::Standard)),
            spi: self.spi_config.map(|config| {
                Spim::new_txonly(
                    config.interface,
                    config.interrupt,
                    config.clock_pin,
                    config.mosi_pin,
                    config.config,
                )
            }),
        }
    }
}

pub struct ScanPins<'a, const C: usize, const R: usize> {
    columns: [Output<'a, AnyPin>; C],
    rows: [Input<'a, AnyPin>; R],
    power_pin: Option<Output<'a, AnyPin>>,
    spi: Option<Spim<'a, SPI3>>,
}

impl HidService {
    pub fn new(sd: &mut Softdevice) -> Result<Self, RegisterError> {
        const PROTOCOL_MODE: Uuid = Uuid::new_16(0x2A4E);
        const REPORT: Uuid = Uuid::new_16(0x2A4D);
        const REPORT_REF_DESCRIPTOR: Uuid = Uuid::new_16(0x2908);

        const PROTOCOL_MODE_BOOT: u8 = 0x0;
        const PROTOCOL_MODE_REPORT: u8 = 0x1;

        let mut service_builder = ServiceBuilder::new(sd, HID_SERVICE)?;

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

    pub fn send_input_report<T>(&self, connection: &Connection, key_state: u64)
    where
        T: Keyboard,
        [(); <T as Scannable>::COLUMNS * <T as Scannable>::ROWS]:,
    {
        const SCAN_CODE_POSITION: usize = 2;
        const REPORT_SIZE: usize = 8;

        let mut input_report = [0; REPORT_SIZE];
        let mut offset = SCAN_CODE_POSITION;

        for index in 0..64 {
            if (key_state >> index) & 0b1 != 0 {
                if offset == REPORT_SIZE {
                    input_report[SCAN_CODE_POSITION..REPORT_SIZE].fill(ERR_OVF.keycode());
                    break;
                }

                let key = T::LAYER_LOOKUP[0][T::MATRIX[index]].keycode();
                input_report[offset] = key;
                offset += 1;
            }
        }

        info!("Sending input report with value {:?}", input_report);

        gatt_server::notify_value(
            connection,
            self.input_report.characteristic_handles.value_handle,
            &input_report,
        )
        .unwrap();
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

// Make this a builder struct so we can make the softdevice reference nice
struct Server<'a> {
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

    pub fn send_input_report<T>(&self, connection: &Connection, key_state: u64)
    where
        T: Keyboard,
        [(); <T as Scannable>::COLUMNS * <T as Scannable>::ROWS]:,
    {
        self.hid_service.send_input_report::<T>(connection, key_state);
    }
}

impl<'a> gatt_server::Server for Server<'a> {
    type Event = ();

    fn on_write(
        &self,
        _conn: &Connection,
        handle: u16,
        _op: WriteOp,
        _offset: usize,
        data: &[u8],
    ) -> Option<Self::Event> {
        info!("Write");

        self.battery_service.on_write(handle, data);
        self.device_information_service.on_write(handle, data);
        self.hid_service.on_write(handle, data);

        None
    }

    fn on_deferred_read(&self, handle: u16, offset: usize, reply: DeferredReadReply) -> Option<Self::Event> {
        info!("Deferred read");
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

macro_rules! do_scan {
    ([$($columns:expr),+], $rows:tt) => {{
        let mut state = 0;

        $({
            $columns.set_high();

            do_scan!(@rows state, ${index()}, $rows);

            $columns.set_low();
        })*

        state
    }};
    (@rows $state:expr, $offset:expr, [$($rows:expr),+]) => {
        $(
            $state |= ($rows.is_high() as u64) << ($offset * ${length()} + ${index()});
        )*
    };
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    // First we get the peripherals access crate.
    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = interrupt::Priority::P2;
    config.time_interrupt_priority = interrupt::Priority::P2;
    config.hfclk_source = HfclkSource::ExternalXtal;
    config.lfclk_source = LfclkSource::ExternalXtal;
    let peripherals = embassy_nrf::init(config);

    let mut meboard = Used::new();
    let mut pins = meboard.init_peripherals(peripherals).to_pins();

    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 6,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 256 }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t { attr_tab_size: 32768 }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 3,
            central_role_count: 3,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: b"HelloRust" as *const u8 as _,
            current_len: 9,
            max_len: 9,
            write_perm: unsafe { mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(raw::BLE_GATTS_VLOC_STACK as u8),
        }),
        ..Default::default()
    };

    let sd = Softdevice::enable(&config);
    let mut server = unwrap!(Server::new(sd));
    server.softdevice = Some(sd);
    unwrap!(spawner.spawn(softdevice_task(sd)));

    #[rustfmt::skip]
    let adv_data = &[
        0x02, 0x01, raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8,
        0x03, 0x03, 0x09, 0x18,
        0x0a, 0x09, b'H', b'e', b'l', b'l', b'o', b'R', b'u', b's', b't',
    ];
    #[rustfmt::skip]
    let scan_data = &[
        0x03, 0x03, 0x09, 0x18,
    ];

    static BONDER: StaticCell<Bonder> = StaticCell::new();
    let bonder = BONDER.init(Bonder::default());

    if let Some(spi) = &mut pins.spi {
        spi.write(&[
            //green (0)
            0b11100000, 0b01110000, 0b00111111, 0b00011111, 0b10001111, 0b11000111, 0b11100011, 0b11110001, 0b11111000,
            // red (255)
            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011, 0b10000001, 0b11000000,
            // blue (255)
            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011, 0b10000001, 0b11000000,
        ])
        .await
        .unwrap();
    }

    let mut previous_key_state = 0;

    loop {
        let config = peripheral::Config::default();
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };

        let connection = unwrap!(peripheral::advertise_pairable(sd, adv, &config, bonder).await);

        let run_future = gatt_server::run(&connection, &server, |event| {
            debug!("Event: {:?}", event);
        });
        pin_mut!(run_future);

        loop {
            let timer_future = Timer::after(Duration::from_millis(1));
            pin_mut!(timer_future);

            match select(run_future, timer_future).await {
                Either::Left((result, _)) => {
                    if let Err(error) = result {
                        debug!("gatt_server run exited with error: {:?}", error);
                    }

                    break;
                }
                Either::Right((_, passed_run_future)) => {
                    // we want to write red 255, green 0, blue 255
                    // => 11111111 00000000 11111111
                    // => 110*8 100*8 110*8
                    // => 111111000*8 111000000*8 111111000*8

                    let key_state = do_scan!(
                        [
                            pins.columns[0],
                            pins.columns[1],
                            pins.columns[2],
                            pins.columns[3],
                            pins.columns[4]
                        ],
                        [pins.rows[0], pins.rows[1], pins.rows[2], pins.rows[3]]
                    );

                    if key_state != previous_key_state {
                        server.send_input_report::<Used>(&connection, key_state);
                        previous_key_state = key_state;
                    }

                    /*if key_state != 0 {
                        spi.write(&[
                            //green (0)
                            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011,
                            0b10000001, 0b11000000, // red (255)
                            0b11100000, 0b01110000, 0b00111111, 0b00011111, 0b10001111, 0b11000111, 0b11100011,
                            0b11110001, 0b11111000, // blue (255)
                            0b11100000, 0b01110000, 0b00111111, 0b00011111, 0b10001111, 0b11000111, 0b11100011,
                            0b11110001, 0b11111000,
                        ])
                        .await
                        .unwrap();
                    } else {
                        spi.write(&[
                            //green (0)
                            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011,
                            0b10000001, 0b11000000, // red (255)
                            0b11100000, 0b01110000, 0b00111111, 0b00011111, 0b10001111, 0b11000111, 0b11100011,
                            0b11110001, 0b11111000, // blue (255)
                            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011,
                            0b10000001, 0b11000000,
                        ])
                        .await
                        .unwrap();
                    }*/

                    run_future = passed_run_future;
                }
            }
        }

        /*spi.write(&[
            //green (0)
            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011, 0b10000001, 0b11000000,
            // red (255)
            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011, 0b10000001, 0b11000000,
            // blue (255)
            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011, 0b10000001, 0b11000000,
        ])
        .await
        .unwrap();*/
    }
}

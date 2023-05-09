mod settings;
mod transaction;

use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use nrf_softdevice::ble::{Address, EncryptionInfo, IdentityKey, MasterId};

pub use self::settings::{flash_task, get_settings, initalize_flash, FlashToken};
pub use self::transaction::{FlashOperation, FlashTransaction};
use crate::interface::Keyboard;

// The Bluetooth address 00:00:00:00:00:00 is technically valid but rarely used
// because it is known to cause problems with most operating systems. So we
// assume that any address only consisting of zeros is not valid.
pub const NO_ADDRESS: Address = Address { flags: 0, bytes: [0; 6] };

const FLASH_CHANNEL_SIZE: usize = 10;

static FLASH_OPERATIONS: Channel<ThreadModeRawMutex, FlashOperation, FLASH_CHANNEL_SIZE> = Channel::new();
static SLAVE_FLASH_OPERATIONS: Channel<ThreadModeRawMutex, FlashOperation, FLASH_CHANNEL_SIZE> = Channel::new();

pub type FlashSender = Sender<'static, ThreadModeRawMutex, FlashOperation, FLASH_CHANNEL_SIZE>;
pub type OtherFlashReceiver = Receiver<'static, ThreadModeRawMutex, FlashOperation, FLASH_CHANNEL_SIZE>;

pub fn flash_sender() -> FlashSender {
    FLASH_OPERATIONS.sender()
}

pub fn other_flash_receiver() -> OtherFlashReceiver {
    SLAVE_FLASH_OPERATIONS.receiver()
}

#[repr(C)]
#[derive(Clone, Copy, Debug, defmt::Format)]
pub struct BondSlot(pub usize);

#[repr(C)]
#[derive(Clone, Copy, Debug, defmt::Format)]
pub struct SystemAttributes {
    pub length: usize,
    pub data: [u8; 64],
}

impl SystemAttributes {
    pub const fn new() -> Self {
        Self { length: 0, data: [0; 64] }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, defmt::Format)]
pub struct Peer {
    pub master_id: MasterId,
    pub key: EncryptionInfo,
    pub peer_id: IdentityKey,
}

#[repr(C)]
#[derive(Clone, Copy, defmt::Format)]
pub struct Bond {
    pub peer: Peer,
    pub system_attributes: SystemAttributes,
}

#[repr(C)]
#[derive(Clone, defmt::Format)]
pub struct Settings {
    pub bonds: [Bond; <crate::Used as Keyboard>::MAXIMUM_BONDS],
    pub board_flash: <crate::Used as Keyboard>::BoardFlash,
}

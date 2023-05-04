use core::mem::MaybeUninit;

use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use futures::pin_mut;
use nrf_softdevice::ble::{Address, EncryptionInfo, FixedGattValue, IdentityKey, MasterId};
use nrf_softdevice::Flash;

use crate::interface::Keyboard;
#[cfg(feature = "lighting")]
use crate::led::{led_sender, Animation};

// The Bluetooth address 00:00:00:00:00:00 is technically valid but rarely used
// because it is known to cause problems with most operating systems. So we
// assume that any address only consisting of zeros is not valid.
pub const NO_ADDRESS: Address = Address { flags: 0, bytes: [0; 6] };

const FLASH_CHANNEL_SIZE: usize = 10;
static FLASH_OPERATIONS: Channel<ThreadModeRawMutex, FlashOperation, FLASH_CHANNEL_SIZE> = Channel::new();
static SLAVE_FLASH_OPERATIONS: Channel<ThreadModeRawMutex, FlashOperation, FLASH_CHANNEL_SIZE> = Channel::new();

pub type FlashSender = Sender<'static, ThreadModeRawMutex, FlashOperation, FLASH_CHANNEL_SIZE>;
pub type SlaveFlashReceiver = Receiver<'static, ThreadModeRawMutex, FlashOperation, FLASH_CHANNEL_SIZE>;

pub fn flash_sender() -> FlashSender {
    FLASH_OPERATIONS.sender()
}

pub fn slave_flash_receiver() -> SlaveFlashReceiver {
    SLAVE_FLASH_OPERATIONS.receiver()
}

pub struct FlashTransaction<const N: usize> {
    operations: [FlashOperation; N],
}

impl FlashTransaction<0> {
    pub fn new() -> Self {
        Self { operations: [] }
    }
}

impl<const N: usize> FlashTransaction<N> {
    fn queue_inner(self, operation: FlashOperation) -> FlashTransaction<{ N + 1 }> {
        let mut operations: [FlashOperation; N + 1] = unsafe { MaybeUninit::zeroed().assume_init() };
        operations[0..N].copy_from_slice(&self.operations);
        operations[N] = operation;
        FlashTransaction { operations }
    }

    #[must_use = "A FlashTransaction needs to be applied in order to do anything"]
    pub fn store_peer(self, slot: BondSlot, peer: Peer) -> FlashTransaction<{ N + 1 }> {
        self.queue_inner(FlashOperation::StorePeer { slot, peer })
    }

    #[must_use = "A FlashTransaction needs to be applied in order to do anything"]
    pub fn store_system_attributes(self, slot: BondSlot, system_attributes: SystemAttributes) -> FlashTransaction<{ N + 1 }> {
        self.queue_inner(FlashOperation::StoreSystemAttributes { slot, system_attributes })
    }

    #[must_use = "A FlashTransaction needs to be applied in order to do anything"]
    pub fn remove_bond(self, slot: BondSlot) -> FlashTransaction<{ N + 1 }> {
        self.queue_inner(FlashOperation::RemoveBond(slot))
    }

    #[cfg(feature = "lighting")]
    #[must_use = "A FlashTransaction needs to be applied in order to do anything"]
    pub fn switch_animation(self, animation: Animation) -> FlashTransaction<{ N + 1 }> {
        self.queue_inner(FlashOperation::SwitchAnimation(animation))
    }

    #[must_use = "A FlashTransaction needs to be applied in order to do anything"]
    pub fn store_board_flash(self, board_flash: <crate::Used as Keyboard>::BoardFlash) -> FlashTransaction<{ N + 1 }> {
        self.queue_inner(FlashOperation::StoreBoardFlash(board_flash))
    }

    pub async fn apply(self) {
        for operation in self.operations.into_iter().chain(core::iter::once(FlashOperation::Apply)) {
            FLASH_OPERATIONS.sender().send(operation).await;
            SLAVE_FLASH_OPERATIONS.sender().send(operation).await;
        }
    }

    pub fn try_apply(self) {
        for operation in self.operations.into_iter().chain(core::iter::once(FlashOperation::Apply)) {
            if FLASH_OPERATIONS.sender().try_send(operation).is_err() {
                defmt::error!("Failed to send flash operation to flash task");
            }

            if SLAVE_FLASH_OPERATIONS.sender().try_send(operation).is_err() {
                defmt::error!("Failed to send flash operation to slave");
            }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, defmt::Format)]
pub struct BondSlot(pub usize);

#[repr(C)]
#[derive(Clone, Copy, Debug, defmt::Format)]
pub enum FlashOperation {
    StorePeer {
        slot: BondSlot,
        peer: Peer,
    },
    StoreSystemAttributes {
        slot: BondSlot,
        system_attributes: SystemAttributes,
    },
    RemoveBond(BondSlot),
    #[cfg(feature = "lighting")]
    SwitchAnimation(Animation),
    StoreBoardFlash(<crate::Used as Keyboard>::BoardFlash),
    Apply,
}

impl FixedGattValue for FlashOperation {
    const SIZE: usize = core::mem::size_of::<FlashOperation>();

    fn from_gatt(data: &[u8]) -> Self {
        let mut buffer = [0; Self::SIZE];
        buffer.copy_from_slice(data);
        unsafe { core::mem::transmute::<&[u8; Self::SIZE], &FlashOperation>(&buffer).clone() }
    }

    fn to_gatt(&self) -> &[u8] {
        unsafe { core::mem::transmute::<&FlashOperation, &[u8; Self::SIZE]>(self) }
    }
}

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
#[derive(Clone, Copy, defmt::Format)]
pub struct FlashSettings {
    pub bonds: [Bond; <crate::Used as Keyboard>::MAXIMUM_BONDS],
    #[cfg(feature = "lighting")]
    pub animation: Animation,
    pub board_flash: <crate::Used as Keyboard>::BoardFlash,
}

mod token {
    use core::mem::MaybeUninit;

    use elain::Align;
    use embassy_nrf::nvmc;
    use embedded_storage_async::nor_flash::{NorFlash, ReadNorFlash};
    use nrf_softdevice::Flash;

    use super::FlashSettings;

    const SETTINGS_PAGES: usize = 1;
    // The flash write needs to be aligned, so we use this wrapper struct
    const PADDING: usize = 3 - ((core::mem::size_of::<FlashSettings>() - 1) % 4);

    #[repr(C)]
    struct ReservedFlash {
        _align: Align<{ nvmc::PAGE_SIZE }>,
        _data: [u8; nvmc::PAGE_SIZE * SETTINGS_PAGES],
    }

    #[repr(C)]
    #[derive(Clone, Copy, defmt::Format)]
    pub(super) struct AlignedFlashSettings {
        pub settings: FlashSettings,
        pub padding: [u8; PADDING],
    }

    #[link_section = ".flash_storage"]
    static RESERVED_FLASH: MaybeUninit<ReservedFlash> = MaybeUninit::uninit();
    static mut SETTINGS_FLASH: MaybeUninit<AlignedFlashSettings> = MaybeUninit::uninit();

    // Assert that the settings are not too big for the flash.
    const _: () = assert!(
        core::mem::size_of::<AlignedFlashSettings>() < core::mem::size_of::<ReservedFlash>(),
        "FlashSettings struct is too big to be stored in the reserved flash. Try making it smaller or reserve more space by adjusting \
         SETTINGS_PAGES."
    );

    // The FlashToken can only be constructed by calling initialize_flash. It is
    // required for every read and write from and to the flash, thereby guaranteeing
    // that the flash has been initialized before we use it.
    #[derive(Copy, Clone, Debug)]
    pub struct FlashToken {
        pub address: u32,
    }

    pub async fn initalize_flash(flash: &mut Flash) -> FlashToken {
        let address = &RESERVED_FLASH as *const _ as u32;
        defmt::debug!("Settings flash is at address 0x{:x}", &RESERVED_FLASH as *const _);

        // Load bytes from flash.
        let mut buffer = [0u8; core::mem::size_of::<AlignedFlashSettings>()];
        defmt::unwrap!(flash.read(address, &mut buffer).await);

        // Save to static variable so that other tasks can read from it.
        let settings =
            unsafe { core::mem::transmute::<&[u8; core::mem::size_of::<AlignedFlashSettings>()], &AlignedFlashSettings>(&buffer) };
        unsafe { SETTINGS_FLASH.write(*settings) };

        // Return a FlashToken that can be used to access the settings.
        FlashToken { address }
    }

    pub fn get_settings(_token: FlashToken) -> &'static FlashSettings {
        // This is perfectly safe since having the FlashToken means that the flash is
        // initalized.
        unsafe { &SETTINGS_FLASH.assume_init_mut().settings }
    }

    pub(super) fn get_aligend_settings(_token: FlashToken) -> &'static mut AlignedFlashSettings {
        // This is perfectly safe since having the FlashToken means that the flash is
        // initalized.
        unsafe { SETTINGS_FLASH.assume_init_mut() }
    }

    pub(super) async fn write_to_flash(flash: &mut Flash, flash_token: FlashToken, settings: &AlignedFlashSettings, erase: bool) {
        let bytes = unsafe { core::mem::transmute::<&AlignedFlashSettings, &[u8; core::mem::size_of::<AlignedFlashSettings>()]>(settings) };

        if erase {
            defmt::trace!("erasing page");
            defmt::unwrap!(flash.erase(flash_token.address, flash_token.address + nvmc::PAGE_SIZE as u32).await);
        }

        defmt::trace!("writing with value: {:#?}", bytes);
        defmt::unwrap!(flash.write(flash_token.address, bytes).await);
    }
}

pub use self::token::{get_settings, initalize_flash, FlashToken};

#[embassy_executor::task]
pub async fn flash_task(flash: Flash, token: FlashToken) {
    let settings = token::get_aligend_settings(token);
    let receiver = FLASH_OPERATIONS.receiver();

    // Led sender
    #[cfg(feature = "lighting")]
    let led_sender = led_sender();

    pin_mut!(flash);

    bitflags::bitflags! {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        struct ApplyFlags: u32 {
            const NONE = 0;
            const ERASE = 0b00000001;
            const WRITE = 0b00000010;
            const ERASE_AND_WRITE = Self::ERASE.bits() | Self::WRITE.bits();
        }
    }

    let mut apply_flags = ApplyFlags::NONE;

    loop {
        let operation = receiver.recv().await;

        match operation {
            FlashOperation::StorePeer { slot, peer } => {
                settings.settings.bonds[slot.0].peer = peer;

                // Since we are potentially trying to set bits to 1 that are currently 0, we
                // need to erase the section before writing.
                apply_flags |= ApplyFlags::ERASE_AND_WRITE;
            }
            FlashOperation::StoreSystemAttributes { slot, system_attributes } => {
                settings.settings.bonds[slot.0].system_attributes = system_attributes;

                // Since we are potentially trying to set bits to 1 that are currently 0, we
                // need to erase the section before writing.
                apply_flags |= ApplyFlags::ERASE_AND_WRITE;
            }
            FlashOperation::RemoveBond(slot) => {
                if settings.settings.bonds[slot.0].peer.peer_id.addr != NO_ADDRESS {
                    settings.settings.bonds[slot.0] = unsafe { MaybeUninit::zeroed().assume_init() };

                    // Since all we are doing is setting the bits of a peer to 0, we can write
                    // without erasing first.
                    apply_flags |= ApplyFlags::WRITE;
                }
            }
            FlashOperation::SwitchAnimation(animation) => {
                if settings.settings.animation != animation {
                    settings.settings.animation = animation;

                    led_sender.send(animation).await;

                    // Since we are potentially trying to set bits to 1 that are currently 0, we
                    // need to erase the section before writing.
                    apply_flags |= ApplyFlags::ERASE_AND_WRITE;
                }
            }
            FlashOperation::StoreBoardFlash(board_flash) => {
                settings.settings.board_flash = board_flash;

                // Since we are potentially trying to set bits to 1 that are currently 0, we
                // need to erase the section before writing.
                apply_flags |= ApplyFlags::ERASE_AND_WRITE;
            }
            FlashOperation::Apply => {
                if apply_flags.contains(ApplyFlags::WRITE) {
                    let erase = apply_flags.contains(ApplyFlags::ERASE);
                    token::write_to_flash(&mut flash, token, settings, erase).await;
                }
                apply_flags = ApplyFlags::NONE;
            }
        }
    }
}

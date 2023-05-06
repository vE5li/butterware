use core::mem::MaybeUninit;

use elain::Align;
use embassy_nrf::nvmc;
use embedded_storage_async::nor_flash::{NorFlash, ReadNorFlash};
use futures::pin_mut;
use nrf_softdevice::Flash;

use super::FlashSettings;
use crate::flash::{FlashOperation, FLASH_OPERATIONS, NO_ADDRESS};
use crate::interface::Keyboard;
use crate::led::led_sender;

// Struct that perfectly alignes with page boundaries of the flash. Placing this
// into the flash gives us a very simple and clean way to get the address and
// size of the page we use for storing the settings.
#[repr(C)]
struct ReservedFlash {
    _align: Align<{ nvmc::PAGE_SIZE }>,
    _data: [u8; nvmc::PAGE_SIZE * <crate::Used as Keyboard>::SETTINGS_PAGES],
}

// The flash can only write full words, so we need to pad the settings with up
// to 3 bytes.
#[repr(C)]
#[derive(Clone, defmt::Format)]
struct AlignedFlashSettings {
    pub settings: FlashSettings,
    pub padding: [u8; 3 - ((core::mem::size_of::<FlashSettings>() - 1) % 4)],
}

#[link_section = ".flash_storage"]
static RESERVED_FLASH: MaybeUninit<ReservedFlash> = MaybeUninit::uninit();
static mut SETTINGS_FLASH: MaybeUninit<AlignedFlashSettings> = MaybeUninit::uninit();

// Assert that the settings are not too big for the flash.
const _: () = assert!(
    core::mem::size_of::<AlignedFlashSettings>() < core::mem::size_of::<ReservedFlash>(),
    "Settings are too big to be stored in the reserved flash. Try making it smaller or reserve more space by adjusting SETTINGS_PAGES."
);

// The FlashToken can only be constructed by calling initialize_flash. It is
// required for every read and write from and to the flash, thereby guaranteeing
// that the flash has been initialized before we use it.
#[derive(Copy, Clone, Debug)]
pub struct FlashToken {
    address: u32,
}

pub async fn initalize_flash(flash: &mut Flash) -> FlashToken {
    let address = &RESERVED_FLASH as *const _ as u32;
    defmt::debug!("Settings flash is at address 0x{:x}", &RESERVED_FLASH as *const _);

    // Load bytes from flash.
    let mut buffer = [0u8; core::mem::size_of::<AlignedFlashSettings>()];
    defmt::unwrap!(flash.read(address, &mut buffer).await);

    // Save to static variable so that other tasks can read from it.
    let settings = unsafe { core::mem::transmute::<&[u8; core::mem::size_of::<AlignedFlashSettings>()], &AlignedFlashSettings>(&buffer) };
    unsafe { SETTINGS_FLASH.write(settings.clone()) };

    // Return a FlashToken that can be used to access the settings.
    FlashToken { address }
}

pub fn get_settings(_token: FlashToken) -> &'static FlashSettings {
    // This is perfectly safe since having the FlashToken means that the flash is
    // initalized.
    unsafe { &SETTINGS_FLASH.assume_init_ref().settings }
}

fn get_aligend_settings(_token: FlashToken) -> &'static mut AlignedFlashSettings {
    // This is perfectly safe since having the FlashToken means that the flash is
    // initalized.
    unsafe { SETTINGS_FLASH.assume_init_mut() }
}

#[embassy_executor::task]
pub async fn flash_task(flash: Flash, token: FlashToken) {
    bitflags::bitflags! {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        struct ApplyFlags: u32 {
            const NONE = 0;
            const ERASE = 0b00000001;
            const WRITE = 0b00000010;
            const ERASE_AND_WRITE = Self::ERASE.bits() | Self::WRITE.bits();
        }
    }

    let aligned = get_aligend_settings(token);
    let receiver = FLASH_OPERATIONS.receiver();
    let mut apply_flags = ApplyFlags::NONE;

    // Led sender
    #[cfg(feature = "lighting")]
    let led_sender = led_sender();

    pin_mut!(flash);

    loop {
        let operation = receiver.recv().await;

        match operation {
            FlashOperation::StorePeer { slot, peer } => {
                aligned.settings.bonds[slot.0].peer = peer;

                // Since we are potentially trying to set bits to 1 that are currently 0, we
                // need to erase the section before writing.
                apply_flags |= ApplyFlags::ERASE_AND_WRITE;
            }
            FlashOperation::StoreSystemAttributes { slot, system_attributes } => {
                aligned.settings.bonds[slot.0].system_attributes = system_attributes;

                // Since we are potentially trying to set bits to 1 that are currently 0, we
                // need to erase the section before writing.
                apply_flags |= ApplyFlags::ERASE_AND_WRITE;
            }
            FlashOperation::RemoveBond(slot) => {
                if aligned.settings.bonds[slot.0].peer.peer_id.addr != NO_ADDRESS {
                    aligned.settings.bonds[slot.0] = unsafe { MaybeUninit::zeroed().assume_init() };

                    // Since all we are doing is setting the bits of a peer to 0, we can write
                    // without erasing first.
                    apply_flags |= ApplyFlags::WRITE;
                }
            }
            FlashOperation::SwitchAnimation(animation) => {
                if aligned.settings.animation != animation {
                    aligned.settings.animation = animation;

                    led_sender.send(animation).await;

                    // Since we are potentially trying to set bits to 1 that are currently 0, we
                    // need to erase the section before writing.
                    apply_flags |= ApplyFlags::ERASE_AND_WRITE;
                }
            }
            FlashOperation::StoreBoardFlash(board_flash) => {
                aligned.settings.board_flash = board_flash;

                // Since we are potentially trying to set bits to 1 that are currently 0, we
                // need to erase the section before writing.
                apply_flags |= ApplyFlags::ERASE_AND_WRITE;
            }
            FlashOperation::Apply => {
                if apply_flags.contains(ApplyFlags::ERASE) {
                    defmt::trace!("erasing page");
                    defmt::unwrap!(flash.erase(token.address, token.address + nvmc::PAGE_SIZE as u32).await);
                }

                if apply_flags.contains(ApplyFlags::WRITE) {
                    let bytes = unsafe {
                        core::mem::transmute::<&AlignedFlashSettings, &[u8; core::mem::size_of::<AlignedFlashSettings>()]>(aligned)
                    };

                    defmt::trace!("writing with value: {:#?}", bytes);
                    defmt::unwrap!(flash.write(token.address, bytes).await);
                }

                apply_flags = ApplyFlags::NONE;
            }
        }
    }
}

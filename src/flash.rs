use core::mem::MaybeUninit;

use elain::Align;
use embassy_nrf::nvmc;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embedded_storage_async::nor_flash::{NorFlash, ReadNorFlash};
use futures::pin_mut;
use nrf_softdevice::ble::{Address, EncryptionInfo, IdentityKey, MasterId};
use nrf_softdevice::Flash;

const SETTINGS_PAGES: usize = 1;
// The flash write needs to be aligned, so we use this wrapper struct
const PADDING: usize = 3 - ((core::mem::size_of::<FlashSettings>() - 1) % 4);
const MAXIMUM_SAVED_CONNECTIONS: usize = 8;
// Assert that the settings are not too big for the flash.
const _: () = assert!(
    core::mem::size_of::<AlignedFlashSettings>() < core::mem::size_of::<ReservedFlash>(),
    "FlashSettings struct is too big to be stored in the reserved flash. Try making it smaller or reserve more space by adjusting \
     SETTINGS_PAGES."
);

#[link_section = ".flash_storage"]
pub static SETTINGS_FLASH: MaybeUninit<ReservedFlash> = MaybeUninit::uninit();
pub static mut FLASH_SETTINGS: MaybeUninit<AlignedFlashSettings> = MaybeUninit::uninit();
pub static FLASH_OPERATIONS: Channel<ThreadModeRawMutex, FlashOperation, 3> = Channel::new();

pub struct BondSlot(pub usize);

pub enum FlashOperation {
    StorePeer(Peer),
    StoreSystemAttributes {
        slot: BondSlot,
        system_attributes: SystemAttributes,
    },
    RemovePeer(BondSlot),
}

#[repr(C)]
pub struct ReservedFlash {
    _align: Align<{ nvmc::PAGE_SIZE }>,
    _data: [u8; nvmc::PAGE_SIZE * SETTINGS_PAGES],
}

#[repr(C)]
#[derive(Clone, Copy, defmt::Format)]
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
    pub bonds: [Bond; MAXIMUM_SAVED_CONNECTIONS],
}

#[repr(C)]
#[derive(Clone, Copy, defmt::Format)]
pub struct AlignedFlashSettings {
    pub settings: FlashSettings,
    pub padding: [u8; PADDING],
}

async fn write_to_flash(flash: &mut Flash, flash_address: u32, settings: &AlignedFlashSettings, erase: bool) {
    let bytes = unsafe { core::mem::transmute::<&AlignedFlashSettings, &[u8; core::mem::size_of::<AlignedFlashSettings>()]>(settings) };

    if erase {
        defmt::trace!("start erase page");
        defmt::unwrap!(flash.erase(flash_address, flash_address + nvmc::PAGE_SIZE as u32).await);
        defmt::trace!("done erase page");
    }

    defmt::trace!("starting write with value: {:#?}", bytes);
    defmt::unwrap!(flash.write(flash_address, bytes).await);
    defmt::trace!("done with write");
}

#[embassy_executor::task]
pub async fn flash_task(flash: Flash) {
    pin_mut!(flash);

    let receiver = FLASH_OPERATIONS.receiver();

    let address = &SETTINGS_FLASH as *const _ as u32;
    defmt::debug!("Settings flash is at address 0x{:x}", &SETTINGS_FLASH as *const _);

    // Load bytes from flash.
    let mut buffer = [0u8; core::mem::size_of::<AlignedFlashSettings>()];
    defmt::unwrap!(flash.read(address, &mut buffer).await);

    // Save to static variable so that other tasks can read from it.
    let settings = unsafe { core::mem::transmute::<&[u8; core::mem::size_of::<AlignedFlashSettings>()], &AlignedFlashSettings>(&buffer) };
    let settings = unsafe { FLASH_SETTINGS.write(*settings) };

    loop {
        let operation = receiver.recv().await;

        match operation {
            FlashOperation::StorePeer(peer) => {
                let free_slot = settings
                    .settings
                    .bonds
                    .iter_mut()
                    .find(|bond| peer.peer_id.addr != Address::default());

                if let Some(slot) = free_slot {
                    slot.peer = peer;

                    // Since we are potentially trying to set bits to 1 that are currently 0, we
                    // need to erase the section before writing.
                    write_to_flash(&mut flash, address, settings, true).await;
                }
            }
            FlashOperation::StoreSystemAttributes { slot, system_attributes } => {
                settings.settings.bonds[slot.0].system_attributes = system_attributes;

                // Since we are potentially trying to set bits to 1 that are currently 0, we
                // need to erase the section before writing.
                write_to_flash(&mut flash, address, settings, true).await;
            }
            FlashOperation::RemovePeer(slot) => {
                // The Bluetooth address 00:00:00:00:00:00 is technically valid but rarely used
                // because it is known to cause problems with most operating systems. So we
                // assume that any slot with an address only consisting of zeros is empty.
                if settings.settings.bonds[slot.0].peer.peer_id.addr != Address::default() {
                    settings.settings.bonds[slot.0] = unsafe { MaybeUninit::zeroed().assume_init() };

                    // Since all we are doing is setting the bits of a peer to 0, we can write
                    // without erasing first.
                    write_to_flash(&mut flash, address, settings, false).await;
                }
            }
        }
    }
}

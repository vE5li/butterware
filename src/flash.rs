use core::mem::MaybeUninit;

use bytemuck::{Pod, Zeroable};
use elain::Align;
use embassy_nrf::nvmc;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embedded_storage_async::nor_flash::{NorFlash, ReadNorFlash};
use futures::pin_mut;
use nrf_softdevice::ble::Address;
use nrf_softdevice::Flash;

use crate::ble::Peer;

const SETTINGS_PAGES: usize = 1;

pub enum FlashOperation {
    StorePeer(crate::ble::Peer),
    RemovePeer(usize),
}

#[repr(C)]
pub struct SettingsFlash {
    _align: Align<{ nvmc::PAGE_SIZE }>,
    _data: [u8; nvmc::PAGE_SIZE * SETTINGS_PAGES],
}

#[link_section = ".flash_storage"]
pub static SETTINGS_FLASH: MaybeUninit<SettingsFlash> = MaybeUninit::uninit();

// Assert that the settings are not too big for the flash.
const _: () = assert!(
    core::mem::size_of::<AlignedFlashSettings>() < nvmc::PAGE_SIZE * SETTINGS_PAGES,
    "FlashSettings struct is too big to be stored in the reserved flash. Try making it smaller or reserve more space by adjusting \
     SETTINGS_PAGES"
);

const MAXIMUM_SAVED_CONNECTIONS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, defmt::Format, Zeroable, Pod)]
pub struct FlashSettings {
    pub peers: [Peer; MAXIMUM_SAVED_CONNECTIONS],
}

// The flash write needs to be aligned, so we use this wrapper struct
const PADDING: usize = 3 - ((core::mem::size_of::<FlashSettings>() - 1) % 4);

#[repr(C)]
#[derive(Clone, Copy, defmt::Format, Zeroable, Pod)]
pub struct AlignedFlashSettings {
    pub settings: FlashSettings,
    pub padding: [u8; PADDING],
}

pub static mut FLASH_SETTINGS: MaybeUninit<AlignedFlashSettings> = MaybeUninit::uninit();
pub static FLASH_OPERATIONS: Channel<ThreadModeRawMutex, FlashOperation, 3> = Channel::new();

async fn write_to_flash(flash: &mut Flash, flash_address: u32, settings: &AlignedFlashSettings, erase: bool) {
    let bytes = bytemuck::bytes_of(settings);

    if erase {
        defmt::debug!("start erase page");
        defmt::unwrap!(flash.erase(flash_address, flash_address + nvmc::PAGE_SIZE as u32).await);
        defmt::debug!("done erase page");
    }

    defmt::debug!("starting write with value: {:#?}", bytes);
    defmt::unwrap!(flash.write(flash_address, bytes).await);
    defmt::debug!("done with write");
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
    let settings = bytemuck::pod_read_unaligned(&buffer);
    let settings = unsafe { FLASH_SETTINGS.write(settings) };

    loop {
        let operation = receiver.recv().await;

        match operation {
            FlashOperation::StorePeer(peer) => {
                settings.settings.peers[0] = peer;

                // Since we are potentially trying to set bits to 1 that are currently 0, we
                // need to erase the section before writing.
                write_to_flash(&mut flash, address, settings, true).await;
            }
            FlashOperation::RemovePeer(slot) => {
                // The Bluetooth address 00:00:00:00:00:00 is technically valid but rarely used
                // because it is known to cause problems with most operating systems. So we
                // assume that any slot with an address only consisting of zeros is empty.
                if settings.settings.peers[slot].peer_id.addr != Address::zeroed() {
                    settings.settings.peers[slot] = Peer::zeroed();

                    // Since all we are doing is setting the bits of a peer to 0, we can write
                    // without erasing first.
                    write_to_flash(&mut flash, address, settings, false).await;
                }
            }
        }
    }
}

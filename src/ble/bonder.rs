use core::cell::{Cell, RefCell};

use bytemuck::{Pod, Zeroable};
use nrf_softdevice::ble::gatt_server::set_sys_attrs;
use nrf_softdevice::ble::security::{IoCapabilities, SecurityHandler};
use nrf_softdevice::ble::{gatt_server, Connection, EncryptionInfo, IdentityKey, MasterId};

use crate::flash::{self, FlashOperation, FLASH_SETTINGS};

#[repr(C)]
#[derive(Debug, Clone, Copy, defmt::Format, Zeroable, Pod)]
pub struct Peer {
    pub master_id: MasterId,
    pub key: EncryptionInfo,
    pub peer_id: IdentityKey,
}

pub struct Bonder {
    peer: Cell<Option<Peer>>,
    sys_attrs: RefCell<heapless::Vec<u8, 62>>,
    sender: embassy_sync::channel::Sender<'static, embassy_sync::blocking_mutex::raw::ThreadModeRawMutex, crate::flash::FlashOperation, 3>,
}

impl Bonder {
    pub fn new() -> Self {
        Self {
            peer: Cell::new(None),
            sys_attrs: Default::default(),
            sender: flash::FLASH_OPERATIONS.sender(),
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
        defmt::info!("The passkey is \"{:a}\"", passkey)
    }

    fn on_bonded(&self, _conn: &Connection, master_id: MasterId, key: EncryptionInfo, peer_id: IdentityKey) {
        defmt::debug!("Storing bond with key {} for master with id {}", key, master_id);

        let peer = Peer { master_id, key, peer_id };

        if self.sender.try_send(FlashOperation::StorePeer(peer)).is_err() {
            defmt::error!("Failed to send flash operation");
        }
    }

    fn get_key(&self, _conn: &Connection, master_id: MasterId) -> Option<EncryptionInfo> {
        let key = unsafe { FLASH_SETTINGS.assume_init_ref() }
            .settings
            .peers
            .iter()
            .find(|peer| peer.master_id == master_id)
            .map(|peer| peer.key);

        match key {
            Some(key) => defmt::debug!("Found key {} for master with id {}", key, master_id),
            None => defmt::debug!("Key for master with id {} not found", master_id),
        }

        key
    }

    fn save_sys_attrs(&self, conn: &Connection) {
        defmt::debug!("saving system attributes for: {}", conn.peer_address());

        if let Some(peer) = self.peer.get() {
            if peer.peer_id.is_match(conn.peer_address()) {
                let mut sys_attrs = self.sys_attrs.borrow_mut();
                let capacity = sys_attrs.capacity();
                defmt::unwrap!(sys_attrs.resize(capacity, 0));
                let len = defmt::unwrap!(gatt_server::get_sys_attrs(conn, &mut sys_attrs)) as u16;
                sys_attrs.truncate(usize::from(len));
                // In a real application you would want to signal another task
                // to permanently store sys_attrs for this connection's peer
            }
        }
    }

    fn load_sys_attrs(&self, conn: &Connection) {
        let addr = conn.peer_address();
        defmt::debug!("loading system attributes for: {}", addr);

        let attrs = self.sys_attrs.borrow();
        // In a real application you would search all stored peers to find a match
        let attrs = if self.peer.get().map(|peer| peer.peer_id.is_match(addr)).unwrap_or(false) {
            (!attrs.is_empty()).then_some(attrs.as_slice())
        } else {
            None
        };

        defmt::unwrap!(set_sys_attrs(conn, attrs));
    }
}

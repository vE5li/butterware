use core::cell::{Cell, RefCell};

use nrf_softdevice::ble::gatt_server::set_sys_attrs;
use nrf_softdevice::ble::security::{IoCapabilities, SecurityHandler};
use nrf_softdevice::ble::{gatt_server, Connection, EncryptionInfo, IdentityKey, MasterId};

use crate::flash::{self, BondSlot, FlashOperation, Peer, SystemAttributes, FLASH_SETTINGS, NO_ADDRESS};

pub struct Bonder {
    peer: Cell<Option<Peer>>,
    sys_attrs: RefCell<heapless::Vec<u8, 62>>,
    sender: embassy_sync::channel::Sender<'static, embassy_sync::blocking_mutex::raw::ThreadModeRawMutex, crate::flash::FlashOperation, 3>,
    slave_sender:
        embassy_sync::channel::Sender<'static, embassy_sync::blocking_mutex::raw::ThreadModeRawMutex, crate::flash::FlashOperation, 3>,
}

impl Bonder {
    pub fn new() -> Self {
        Self {
            peer: Cell::new(None),
            sys_attrs: Default::default(),
            sender: flash::FLASH_OPERATIONS.sender(),
            slave_sender: flash::SLAVE_FLASH_OPERATIONS.sender(),
        }
    }
}

impl Bonder {
    fn send(&self, flash_operation: FlashOperation) {
        if self.sender.try_send(flash_operation).is_err() {
            defmt::error!("Failed to send flash operation to flash task");
        }

        if self.slave_sender.try_send(flash_operation).is_err() {
            defmt::error!("Failed to send flash operation to slave");
        }

        defmt::warn!("Sent to flash_operations");
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

        let free_slot = unsafe { FLASH_SETTINGS.assume_init_ref() }
            .settings
            .bonds
            .iter()
            .position(|bond| bond.peer.peer_id.addr == NO_ADDRESS);

        match free_slot {
            Some(free_slot) => defmt::trace!("Found free slot at {}", free_slot),
            None => defmt::trace!("No free slot found for bond"),
        }

        // FIX: Figure out how to choose another slot if all of them are full
        if let Some(free_slot) = free_slot {
            let peer = Peer { master_id, key, peer_id };
            let flash_operation = FlashOperation::StorePeer {
                slot: BondSlot(free_slot),
                peer,
            };
            self.send(flash_operation);
        }
    }

    fn get_key(&self, _conn: &Connection, master_id: MasterId) -> Option<EncryptionInfo> {
        let key = unsafe { FLASH_SETTINGS.assume_init_ref() }
            .settings
            .bonds
            .iter()
            .find(|bond| bond.peer.master_id == master_id)
            .map(|bond| bond.peer.key);

        match key {
            Some(key) => defmt::trace!("Found key {} for master with id {}", key, master_id),
            None => defmt::trace!("Key for master with id {} not found", master_id),
        }

        key
    }

    fn save_sys_attrs(&self, conn: &Connection) {
        let peer_address = conn.peer_address();

        defmt::debug!("Saving system attributes for peer with address {}", peer_address);

        let slot = unsafe { FLASH_SETTINGS.assume_init_ref() }
            .settings
            .bonds
            .iter()
            .position(|bond| bond.peer.peer_id.addr == peer_address);

        match slot {
            Some(slot) => defmt::trace!("Found bond for peer with address {} in slot {}", peer_address, slot),
            None => defmt::trace!("No bond found for peer with address {}", peer_address),
        }

        if let Some(slot) = slot {
            let mut system_attributes = SystemAttributes::new();
            let length = defmt::unwrap!(gatt_server::get_sys_attrs(conn, &mut system_attributes.data));
            system_attributes.length = length;

            let flash_operation = FlashOperation::StoreSystemAttributes {
                slot: BondSlot(slot),
                system_attributes,
            };
            self.send(flash_operation);
        }
    }

    fn load_sys_attrs(&self, conn: &Connection) {
        let peer_address = conn.peer_address();

        defmt::debug!("Loading system attributes for peer with address {}", peer_address);

        let attributes = unsafe { FLASH_SETTINGS.assume_init_ref() }
            .settings
            .bonds
            .iter()
            .find(|bond| bond.peer.peer_id.addr == peer_address)
            .map(|bond| &bond.system_attributes.data[..bond.system_attributes.length])
            .filter(|attributes| !attributes.is_empty());

        match attributes {
            Some(attributes) => defmt::trace!(
                "Found system attributes {:?} for peer with address {}",
                attributes,
                peer_address
            ),
            None => defmt::trace!("No system attributes found for peer with address {}", peer_address),
        }

        defmt::unwrap!(set_sys_attrs(conn, attributes));
    }
}

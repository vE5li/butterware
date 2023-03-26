use nrf_softdevice::ble::FixedGattValue;

use super::{BondSlot, Peer, SystemAttributes, FLASH_OPERATIONS, SLAVE_FLASH_OPERATIONS};
use crate::interface::Keyboard;
use crate::Side;

#[repr(C)]
#[derive(Clone, defmt::Format)]
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
    StoreBoardFlash(<crate::Used as Keyboard>::BoardFlash),
    ResetPersistentData,
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

async fn queue_inner(side: Side, operation: FlashOperation) {
    if side.includes_this() {
        FLASH_OPERATIONS.send(operation.clone()).await;
    }

    if side.includes_other() {
        SLAVE_FLASH_OPERATIONS.send(operation).await;
    }
}

pub async fn remove_bond(side: Side, slot: BondSlot) {
    queue_inner(side, FlashOperation::RemoveBond(slot)).await;
}

pub async fn store_board_flash(side: Side, board_flash: <crate::Used as Keyboard>::BoardFlash) {
    queue_inner(side, FlashOperation::StoreBoardFlash(board_flash)).await;
}

pub async fn reset_persistent_data(side: Side) {
    queue_inner(side, FlashOperation::ResetPersistentData).await;
}

// TODO: make these async when the bonder is async

fn try_queue_inner(side: Side, operation: FlashOperation) {
    if side.includes_this() && FLASH_OPERATIONS.try_send(operation.clone()).is_err() {
        defmt::error!("Failed to send flash operation to flash task");
    }

    if side.includes_other() && SLAVE_FLASH_OPERATIONS.try_send(operation).is_err() {
        defmt::error!("Failed to send flash operation to slave");
    }
}

pub fn try_store_peer(side: Side, slot: BondSlot, peer: Peer) {
    try_queue_inner(side, FlashOperation::StorePeer { slot, peer });
}

pub fn try_store_system_attributes(side: Side, slot: BondSlot, system_attributes: SystemAttributes) {
    try_queue_inner(side, FlashOperation::StoreSystemAttributes { slot, system_attributes });
}

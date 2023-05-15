use core::mem::MaybeUninit;

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

pub struct FlashTransaction<const N: usize> {
    operations: [(Side, FlashOperation); N],
}

impl FlashTransaction<0> {
    pub fn new() -> Self {
        Self { operations: [] }
    }
}

impl<const N: usize> FlashTransaction<N> {
    fn queue_inner(self, side: Side, operation: FlashOperation) -> FlashTransaction<{ N + 1 }> {
        let mut operations: [(Side, FlashOperation); N + 1] = unsafe { MaybeUninit::zeroed().assume_init() };
        operations[0..N].clone_from_slice(&self.operations);
        operations[N] = (side, operation);
        FlashTransaction { operations }
    }

    #[must_use = "A FlashTransaction needs to be applied in order to do anything"]
    pub fn store_peer(self, side: Side, slot: BondSlot, peer: Peer) -> FlashTransaction<{ N + 1 }> {
        self.queue_inner(side, FlashOperation::StorePeer { slot, peer })
    }

    #[must_use = "A FlashTransaction needs to be applied in order to do anything"]
    pub fn store_system_attributes(self, side: Side, slot: BondSlot, system_attributes: SystemAttributes) -> FlashTransaction<{ N + 1 }> {
        self.queue_inner(side, FlashOperation::StoreSystemAttributes { slot, system_attributes })
    }

    #[must_use = "A FlashTransaction needs to be applied in order to do anything"]
    pub fn remove_bond(self, side: Side, slot: BondSlot) -> FlashTransaction<{ N + 1 }> {
        self.queue_inner(side, FlashOperation::RemoveBond(slot))
    }

    #[must_use = "A FlashTransaction needs to be applied in order to do anything"]
    pub fn store_board_flash(self, side: Side, board_flash: <crate::Used as Keyboard>::BoardFlash) -> FlashTransaction<{ N + 1 }> {
        self.queue_inner(side, FlashOperation::StoreBoardFlash(board_flash))
    }

    #[must_use = "A FlashTransaction needs to be applied in order to do anything"]
    pub fn reset_persistent_data(self, side: Side) -> FlashTransaction<{ N + 1 }> {
        self.queue_inner(side, FlashOperation::ResetPersistentData)
    }

    pub async fn apply(self) {
        for (side, operation) in self
            .operations
            .into_iter()
            .chain(core::iter::once((Side::Both, FlashOperation::Apply)))
        {
            if side.includes_this() {
                FLASH_OPERATIONS.send(operation.clone()).await;
            }

            if side.includes_other() {
                SLAVE_FLASH_OPERATIONS.send(operation).await;
            }
        }
    }

    pub fn try_apply(self) {
        for (side, operation) in self
            .operations
            .into_iter()
            .chain(core::iter::once((Side::Both, FlashOperation::Apply)))
        {
            if side.includes_this() && FLASH_OPERATIONS.try_send(operation.clone()).is_err() {
                defmt::error!("Failed to send flash operation to flash task");
            }

            if side.includes_other() && SLAVE_FLASH_OPERATIONS.try_send(operation).is_err() {
                defmt::error!("Failed to send flash operation to slave");
            }
        }
    }
}

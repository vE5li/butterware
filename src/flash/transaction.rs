use core::mem::MaybeUninit;

use nrf_softdevice::ble::FixedGattValue;

use super::{BondSlot, Peer, SystemAttributes, FLASH_OPERATIONS, SLAVE_FLASH_OPERATIONS};
use crate::interface::Keyboard;
#[cfg(feature = "lighting")]
use crate::led::Animation;

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
        operations[0..N].clone_from_slice(&self.operations);
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
            FLASH_OPERATIONS.sender().send(operation.clone()).await;
            SLAVE_FLASH_OPERATIONS.sender().send(operation).await;
        }
    }

    pub fn try_apply(self) {
        for operation in self.operations.into_iter().chain(core::iter::once(FlashOperation::Apply)) {
            if FLASH_OPERATIONS.sender().try_send(operation.clone()).is_err() {
                defmt::error!("Failed to send flash operation to flash task");
            }

            if SLAVE_FLASH_OPERATIONS.sender().try_send(operation).is_err() {
                defmt::error!("Failed to send flash operation to slave");
            }
        }
    }
}

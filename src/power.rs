use embassy_nrf::gpio::{AnyPin, Output};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use nrf_softdevice::ble::FixedGattValue;

use crate::Side;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, defmt::Format)]
pub enum PowerState {
    Off,
    On,
}

impl core::ops::Not for PowerState {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            PowerState::Off => PowerState::On,
            PowerState::On => PowerState::Off,
        }
    }
}

#[repr(C)]
#[derive(Clone, defmt::Format)]
pub enum PowerOperation {
    SetPower { state: PowerState },
}

impl FixedGattValue for PowerOperation {
    const SIZE: usize = core::mem::size_of::<PowerOperation>();

    fn from_gatt(data: &[u8]) -> Self {
        let mut buffer = [0; Self::SIZE];
        buffer.copy_from_slice(data);
        unsafe { core::mem::transmute::<&[u8; Self::SIZE], &PowerOperation>(&buffer).clone() }
    }

    fn to_gatt(&self) -> &[u8] {
        unsafe { core::mem::transmute::<&PowerOperation, &[u8; Self::SIZE]>(self) }
    }
}
pub async fn set_power_state(side: Side, state: PowerState) {
    let power_operation = PowerOperation::SetPower { state };

    if side.includes_this() {
        POWER_OPERATIONS.send(power_operation.clone()).await;
    }

    if side.includes_other() {
        OTHER_POWER_OPERATIONS.send(power_operation).await;
    }
}

const POWER_CHANNEL_SIZE: usize = 10;

pub type PowerSender = Sender<'static, ThreadModeRawMutex, PowerOperation, POWER_CHANNEL_SIZE>;
pub type OtherPowerReceiver = Receiver<'static, ThreadModeRawMutex, PowerOperation, POWER_CHANNEL_SIZE>;

static POWER_OPERATIONS: Channel<ThreadModeRawMutex, PowerOperation, POWER_CHANNEL_SIZE> = Channel::new();
static OTHER_POWER_OPERATIONS: Channel<ThreadModeRawMutex, PowerOperation, POWER_CHANNEL_SIZE> = Channel::new();

pub fn power_sender() -> PowerSender {
    POWER_OPERATIONS.sender()
}

pub fn other_power_receiver() -> OtherPowerReceiver {
    OTHER_POWER_OPERATIONS.receiver()
}

#[embassy_executor::task]
pub async fn power_task(mut power_pin: Option<Output<'static, AnyPin>>) -> ! {
    let receiver = POWER_OPERATIONS.receiver();

    loop {
        let power_operation = receiver.recv().await;

        match power_operation {
            PowerOperation::SetPower { state } => {
                if let Some(power_pin) = &mut power_pin {
                    match state {
                        PowerState::On => power_pin.set_high(),
                        PowerState::Off => power_pin.set_low(),
                    }
                }
            }
        }
    }
}

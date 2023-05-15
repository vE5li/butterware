use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};

use crate::interface::Keyboard;
use crate::Side;

const EVENTS_CHANNEL_SIZE: usize = 10;

pub type UsedEvent = <crate::Used as Keyboard>::Events;
pub type EventSender = Sender<'static, ThreadModeRawMutex, UsedEvent, EVENTS_CHANNEL_SIZE>;
pub type EventReceiver = Receiver<'static, ThreadModeRawMutex, UsedEvent, EVENTS_CHANNEL_SIZE>;
pub type OtherEventReceiver = Receiver<'static, ThreadModeRawMutex, UsedEvent, EVENTS_CHANNEL_SIZE>;

static EVENTS: Channel<ThreadModeRawMutex, UsedEvent, EVENTS_CHANNEL_SIZE> = Channel::new();
static OTHER_EVENTS: Channel<ThreadModeRawMutex, UsedEvent, EVENTS_CHANNEL_SIZE> = Channel::new();

pub fn event_sender() -> EventSender {
    EVENTS.sender()
}

pub fn event_receiver() -> EventReceiver {
    EVENTS.receiver()
}

pub fn other_event_receiver() -> OtherEventReceiver {
    OTHER_EVENTS.receiver()
}

pub async fn trigger_event(side: Side, event: UsedEvent) {
    if side.includes_this() {
        EVENTS.send(event.clone()).await;
    }

    if side.includes_other() {
        OTHER_EVENTS.send(event).await;
    }
}

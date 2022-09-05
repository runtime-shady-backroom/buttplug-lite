use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};

use futures::stream::Fuse;
use futures::StreamExt;
use iced_native::{Subscription, subscription};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

type Stream = Fuse<UnboundedReceiverStream<ApplicationStatusEvent>>;

struct Marker;

#[derive(Debug)]
pub enum ApplicationStatusEvent {
    DeviceAdded,
    DeviceRemoved,
    Tick(u32),
}

static NEXT_TICK: AtomicU32 = AtomicU32::new(0);

impl ApplicationStatusEvent {
    pub fn next_tick() -> ApplicationStatusEvent {
        ApplicationStatusEvent::Tick(NEXT_TICK.fetch_add(1, Ordering::Relaxed))
    }
}

enum State {
    Ready(Stream),
    Error,
}

pub struct ApplicationStatusSubscriptionProvider {
    stream: RefCell<Option<Stream>>,
}

impl ApplicationStatusSubscriptionProvider {
    pub fn new(
        receiver: mpsc::UnboundedReceiver<ApplicationStatusEvent>) -> ApplicationStatusSubscriptionProvider
    {
        let stream = UnboundedReceiverStream::new(receiver).fuse();

        ApplicationStatusSubscriptionProvider {
            stream: RefCell::new(Some(stream)),
        }
    }

    pub fn subscribe(&self) -> Subscription<ApplicationStatusEvent> {
        // initial state must be set up outside of the unfold, because we can't pass &self into the closure
        // if iced ever somehow initializes the unfold twice it'll put us in an invalid Error state.
        let initial_state = match self.stream.take() {
            Some(stream) => State::Ready(stream),
            None => State::Error,
        };

        subscription::unfold(
            std::any::TypeId::of::<Marker>(),
            initial_state,
            |state| async move {
                match state {
                    State::Ready(mut stream) => {
                        let event = stream
                            .select_next_some()
                            .await;
                        (Some(event), State::Ready(stream))
                    }
                    State::Error => {
                        panic!("The subscription ended up in the error state")
                    }
                }
            },
        )
    }
}

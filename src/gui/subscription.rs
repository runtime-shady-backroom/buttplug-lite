// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use std::cell::RefCell;

use futures::stream::{Fuse, StreamExt as _};
use iced::{Subscription, subscription};
use iced_futures::MaybeSend;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

type Stream<T> = Fuse<UnboundedReceiverStream<T>>;

struct Marker;

#[derive(Debug)]
pub enum ApplicationStatusEvent {
    DeviceAdded,
    DeviceRemoved,
    Tick,
}

impl ApplicationStatusEvent {
    pub fn next_tick() -> ApplicationStatusEvent {
        ApplicationStatusEvent::Tick
    }
}

enum State<T> {
    Ready(Stream<T>),
    Error,
}

///
/// `T` is the type of the event
pub struct SubscriptionProvider<T> {
    stream: RefCell<Option<Stream<T>>>,
}

impl <T: MaybeSend + 'static> SubscriptionProvider<T> {
    pub fn new(
        receiver: mpsc::UnboundedReceiver<T>) -> SubscriptionProvider<T>
    {
        let stream = UnboundedReceiverStream::new(receiver).fuse();

        SubscriptionProvider {
            stream: RefCell::new(Some(stream)),
        }
    }

    pub fn subscribe(&self) -> Subscription<T> {
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
                        let event: T = futures::select! {
                            result = stream.select_next_some() => {
                                result
                            }
                        };
                        (event, State::Ready(stream))
                    }
                    State::Error => {
                        panic!("The subscription ended up in the error state")
                    }
                }
            },
        )
    }
}

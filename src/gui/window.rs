// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use std::collections::HashMap;
use std::fmt;
use std::fmt::{Display, Formatter};

use iced::{alignment::Alignment, Application, Command, Element, Length, Settings, Subscription, Theme, theme};
use iced::widget::{Button, Column, Container, Row, Rule, Scrollable, Text, TextInput};
use iced_native::Event;
use semver::Version;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

use crate::{ApplicationStateDb, ShutdownMessage};
use crate::app::buttplug;
use crate::app::structs::{ApplicationStatus, DeviceStatus};
use crate::config::v3::{ConfigurationV3, MotorConfigurationV3, MotorTypeV3};
use crate::gui::constants::*;
use crate::gui::structs::MotorMessage;
use crate::gui::subscription::{ApplicationStatusEvent, SubscriptionProvider};
use crate::gui::tagged_motor::TaggedMotor;
use crate::gui::theme::THEME;
use crate::gui::TokioExecutor;
use crate::gui::util;
use crate::util::slice as slice_util;
use crate::util::update_checker;

pub fn run(
    application_state_db: ApplicationStateDb,
    warp_shutdown_tx: UnboundedSender<ShutdownMessage>,
    initial_devices: ApplicationStatus,
    application_status_subscription: SubscriptionProvider<ApplicationStatusEvent>,
) {
    let settings = Settings {
        id: Some("buttplug-lite".to_string()),
        window: Default::default(),
        flags: Flags {
            warp_restart_tx: warp_shutdown_tx.clone(),
            application_state_db,
            initial_application_status: initial_devices,
            application_status_subscription,
        },
        default_font: Default::default(),
        default_text_size: TEXT_SIZE_DEFAULT,
        antialiasing: true,
        exit_on_close_request: false,
        text_multithreading: false,
        try_opengles_first: false,
    };

    Gui::run(settings).expect("could not instantiate window");
    match warp_shutdown_tx.send(ShutdownMessage::Shutdown) {
        Ok(()) => info!("shutdown triggered by UI close"),
        Err(e) => panic!("Error triggering shutdown: {e}")
    };
}

struct Flags {
    warp_restart_tx: UnboundedSender<ShutdownMessage>,
    application_state_db: ApplicationStateDb,
    initial_application_status: ApplicationStatus,
    application_status_subscription: SubscriptionProvider<ApplicationStatusEvent>,
}

#[derive(Debug, Clone)]
enum Message {
    SaveConfigurationRequest,
    RefreshDevices,
    RefreshDevicesComplete(Option<ApplicationStatus>),
    SaveConfigurationComplete(Result<ConfigurationV3, String>),
    PortUpdated(String),
    MotorMessageContainer(usize, MotorMessage),
    NativeEventOccurred(Event),
    Tick,
    UpdateButtonPressed,
    StartupActionCompleted(StartupActionResult)
}

enum Gui {
    /// intermediate state used for memory-fuckery reasons during transitions
    Invalid,
    Loaded(State),
}

#[derive(Debug, Clone)]
enum UpdateCheck {
    Uninitialized,
    NoUpdateNeeded,
    UpdateNeeded(String),
}

struct State {
    motors: Vec<TaggedMotor>,
    devices: Vec<DeviceStatus>,
    port: u16,
    port_text: String,
    warp_restart_tx: UnboundedSender<ShutdownMessage>,
    application_state_db: ApplicationStateDb,
    configuration_dirty: bool,
    motor_tags_valid: bool,
    saving: bool,
    last_configuration: ConfigurationV3,
    application_status_subscription: SubscriptionProvider<ApplicationStatusEvent>,
    update_check: UpdateCheck,
}

impl Gui {
    fn new(flags: Flags) -> Self {
        let config_version = flags.initial_application_status.configuration.version;
        let port = flags.initial_application_status.configuration.port;
        let ApplicationStatus { motors, devices, configuration } = flags.initial_application_status;

        Gui::Loaded(State {
            devices,
            motors,
            port,
            port_text: port.to_string(),
            warp_restart_tx: flags.warp_restart_tx,
            application_state_db: flags.application_state_db,
            configuration_dirty: ConfigurationV3::is_version_outdated(config_version),
            motor_tags_valid: true,
            saving: false,
            last_configuration: configuration,
            application_status_subscription: flags.application_status_subscription,
            update_check: UpdateCheck::Uninitialized,
        })
    }

    fn on_configuration_changed(&mut self) {
        if let Gui::Loaded(state) = self {
            // what the new configuration would be if we saved now
            let new_configuration = ConfigurationV3::new(state.port, tags_from_application_status(&state.motors));
            state.configuration_dirty = new_configuration != state.last_configuration;
        }
    }
}

impl Application for Gui {
    type Executor = TokioExecutor;
    type Message = Message;
    type Theme = Theme;
    type Flags = Flags;

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (Gui::new(flags), Command::perform(gui_startup_action(), Message::StartupActionCompleted))
    }

    fn title(&self) -> String {
        format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match self {
            Gui::Invalid => {
                panic!("GUI was unexpectedly in an invalid state");
            }
            Gui::Loaded(state) => {
                match message {
                    Message::StartupActionCompleted(result) => {
                        state.update_check = result.update_check;
                        Command::none()
                    }
                    Message::RefreshDevices => {
                        info!("device refresh triggered");
                        Command::perform(get_tagged_devices(state.application_state_db.clone()), Message::RefreshDevicesComplete)
                    }
                    Message::RefreshDevicesComplete(application_status) => {
                        if let Some(application_status) = application_status {
                            // we conduct the ol' switcharoo to move our old state into the new state without having to clone absolutely everything
                            if let Gui::Loaded(old_state) = std::mem::replace(self, Gui::Invalid) {

                                //TODO: something in here nukes the status of motor tags that we're currently editing
                                if old_state.motors != application_status.motors {
                                    debug!("old motors = {:?}", old_state.motors);
                                    debug!("new motors = {:?}", application_status.motors);
                                }

                                *self = Gui::Loaded(State {
                                    devices: application_status.devices,
                                    motors: application_status.motors,
                                    port: old_state.port,
                                    port_text: old_state.port_text,
                                    warp_restart_tx: old_state.warp_restart_tx,
                                    application_state_db: old_state.application_state_db,
                                    configuration_dirty: old_state.configuration_dirty,
                                    motor_tags_valid: old_state.motor_tags_valid,
                                    saving: old_state.saving,
                                    last_configuration: old_state.last_configuration,
                                    application_status_subscription: old_state.application_status_subscription,
                                    update_check: old_state.update_check,
                                });
                            } else {
                                // this should never happen
                                panic!("GUI was unexpectedly in an invalid state");
                            }
                        } else {
                            panic!("Application was unexpectedly not in loaded state");
                        }

                        debug!("Finished handling RefreshDevicesComplete event");
                        Command::none()
                    }
                    Message::SaveConfigurationRequest => {
                        if state.saving {
                            debug!("Save requested but we're already saving! I didn't realize this was possible… but I handled it anyways");
                            Command::none()
                        } else {
                            info!("save initiated");
                            state.saving = true;

                            state.port_text = state.port.to_string();

                            let configuration = ConfigurationV3::new(state.port, tags_from_application_status(&state.motors));
                            Command::perform(update_configuration(state.application_state_db.clone(), configuration, state.warp_restart_tx.clone()), Message::SaveConfigurationComplete)
                        }
                    }
                    Message::SaveConfigurationComplete(result) => {
                        state.saving = false;
                        let application_state = state.application_state_db.clone();
                        match result {
                            Ok(configuration) => {
                                state.last_configuration = configuration;
                                self.on_configuration_changed();
                            }
                            Err(e) => {
                                warn!("save failed: {e:?}");
                            }
                        }

                        // trigger a motor refresh
                        // this is needed because when we hit save we may have cleared old tags that no longer match any existing device
                        Command::perform(get_tagged_devices(application_state), Message::RefreshDevicesComplete)
                    }
                    Message::PortUpdated(new_port) => {
                        state.port_text = new_port;
                        //TODO: notify user if port is invalid
                        state.port = state.port_text.parse::<u16>().unwrap_or(state.port);
                        self.on_configuration_changed();
                        Command::none()
                    }
                    Message::MotorMessageContainer(motor_index, motor_message) => {
                        // this happens BEFORE state.motors is updated with the new information passed via this message

                        // motor indices sorted by the tag they reference
                        let mut indices: Vec<usize> = (0..state.motors.len()).collect();
                        indices.sort_unstable_by_key(|i| override_tag_at_index(&state.motors, *i, motor_index, motor_message.tag()));

                        // find the duplicate indices
                        // note that this will leave one index from each group in the unique portion: we'll fix this later
                        let split_point = slice_util::partition_dedup_by(&mut indices, |index_a, index_b| {
                            if let Some(motor_a_tag) = override_tag_at_index(&state.motors, *index_a, motor_index, motor_message.tag()) {
                                if let Some(motor_b_tag) = override_tag_at_index(&state.motors, *index_b, motor_index, motor_message.tag()) {
                                    motor_a_tag == motor_b_tag
                                } else {
                                    // motor_b had no tag, and the absence of a tag cannot be a duplicate
                                    false
                                }
                            } else {
                                // motor_a had no tag, and the absence of a tag cannot be a duplicate
                                false
                            }
                        });

                        // do a second pass to pull out the rest of the duplicates
                        let (unique_indices, duplicate_indices) = indices.split_at_mut(split_point);
                        let split_point = itertools::partition(unique_indices, |unique_index| {
                            let unique_tag = override_tag_at_index(&state.motors, *unique_index, motor_index, motor_message.tag());
                            !duplicate_indices.iter().any(|duplicate_index| {
                                let duplicate_tag = override_tag_at_index(&state.motors, *duplicate_index, motor_index, motor_message.tag());
                                unique_tag == duplicate_tag
                            })
                        });
                        let (unique_indices, duplicate_indices) = indices.split_at(split_point);

                        // handle each motor with a unique tag
                        let mut tags_valid = true;
                        for unique_index in unique_indices {
                            let tag = override_tag_at_index(&state.motors, *unique_index, motor_index, motor_message.tag()).map(|t| t.to_string());
                            let motor = &mut state.motors[*unique_index];
                            match tag {
                                Some(tag) => {
                                    let valid = is_tag_valid(&tag);
                                    tags_valid &= valid; // any falses need to stick
                                    motor.update(MotorMessage::TagUpdated { tag, valid })
                                }
                                None => motor.update(MotorMessage::TagDeleted),
                            }
                        }

                        // handle each motor with a duplicated tag
                        for duplicate_index in duplicate_indices {
                            // safe to unwrap here as duplicate motors cannot have a missing tag
                            let tag = override_tag_at_index(&state.motors, *duplicate_index, motor_index, motor_message.tag()).unwrap().to_string();
                            let motor = &mut state.motors[*duplicate_index];
                            motor.update(MotorMessage::TagUpdated { tag, valid: false });
                        }

                        state.motor_tags_valid = duplicate_indices.is_empty() && tags_valid;
                        self.on_configuration_changed();
                        Command::none()
                    }
                    Message::NativeEventOccurred(event) => {
                        if let Event::Window(iced_native::window::Event::CloseRequested) = event {
                            info!("received gui shutdown request");
                            iced::window::close()
                        } else {
                            Command::none()
                        }
                    }
                    Message::Tick => {
                        // this should keep battery levels reasonably up to date
                        Command::perform(get_tagged_devices(state.application_state_db.clone()), Message::RefreshDevicesComplete)
                    }
                    Message::UpdateButtonPressed => {
                        if let UpdateCheck::UpdateNeeded(update_url) = &state.update_check {
                            open::that(update_url).expect("Failed to open update URL");
                        } else {
                            panic!("Somehow pressed the update button without it visible!?");
                        }

                        Command::none()
                    }
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        match self {
            Gui::Invalid => {
                panic!("GUI was unexpectedly in an invalid state");
            }
            Gui::Loaded(state) => {
                let example_message = format!("example message: {}", build_example_message(&state.motors));

                let save_button_text = if state.saving {
                    "saving…"
                } else {
                    "save & apply configuration"
                };
                let mut save_button = Button::new(Text::new(save_button_text));
                if save_allowed(state) {
                    save_button = save_button.on_press(Message::SaveConfigurationRequest);
                }

                let content = Scrollable::new(
                    Column::new()
                        .spacing(TABLE_SPACING)
                        .padding(TABLE_SPACING)
                        .width(Length::Fill)
                        .push({
                            let row = Row::new()
                                .spacing(TABLE_SPACING)
                                .push(save_button);
                            if let UpdateCheck::UpdateNeeded(_) = state.update_check {
                                row.push(
                                    Button::new(Text::new("Update Available!"))
                                        .on_press(Message::UpdateButtonPressed)
                                        .style(theme::Button::Destructive)
                                )
                            } else {
                                row
                            }
                        })
                        .push(Row::new()
                            .spacing(EOL_INPUT_SPACING)
                            .align_items(Alignment::Center)
                            .push(util::input_label("Server port:"))
                            .push(
                                TextInput::new("server port", state.port_text.as_str(), Message::PortUpdated)
                                    .width(Length::Fixed(PORT_INPUT_WIDTH))
                                    .padding(TEXT_INPUT_PADDING)
                            )
                        )
                        .push(
                            Rule::horizontal(TABLE_SPACING)
                        )
                        .push(Row::new()
                            .spacing(TABLE_SPACING)
                            .push(
                                render_motor_list(&state.motors)
                            )
                            .push(
                                render_device_list(&state.devices)
                            )
                        )
                        .push(
                            Rule::horizontal(TABLE_SPACING)
                        )
                        .push(Text::new(example_message).size(TEXT_SIZE_SMALL))
                );

                Container::new(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
        }
    }

    fn theme(&self) -> Self::Theme {
        THEME.clone()
    }

    // this is called many times in strange and mysterious ways
    fn subscription(&self) -> Subscription<Message> {
        let native_events = iced_native::subscription::events()
            .map(Message::NativeEventOccurred);

        match self {
            Gui::Loaded(state) => {
                let application_events = state.application_status_subscription.subscribe()
                    .map(|event| match event {
                        ApplicationStatusEvent::DeviceAdded => Message::RefreshDevices,
                        ApplicationStatusEvent::DeviceRemoved => Message::RefreshDevices,
                        ApplicationStatusEvent::Tick(_) => Message::Tick
                    });
                Subscription::batch(vec![application_events, native_events])
            }
            Gui::Invalid => panic!("GUI was unexpectedly in an invalid state"),
        }
    }
}

#[derive(Debug, Clone)]
struct StartupActionResult {
    update_check: UpdateCheck,
}

async fn gui_startup_action() -> StartupActionResult {
    // grab our local version
    let local_version = Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|e| panic!("Local version \"{}\" didn't follow semver! {}", env!("CARGO_PKG_VERSION"), e));
    let update_url = update_checker::check_for_update(local_version).await;
    let update_check = match update_url {
        Some(update_url) => UpdateCheck::UpdateNeeded(update_url),
        None => UpdateCheck::NoUpdateNeeded,
    };
    StartupActionResult { update_check }
}


impl Display for TaggedMotor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.motor, self.tag())
    }
}

fn render_motor_list(motors: &Vec<TaggedMotor>) -> Element<Message> {
    let col = Column::new()
        .spacing(TABLE_SPACING)
        .push(Text::new("Motor Configuration").size(TEXT_SIZE_BIG));
    let col = if motors.is_empty() {
        col.push(Text::new("No motors"))
    } else {
        motors.iter()
            .enumerate()
            .fold(col, |column, (i, motor)| {
                column.push(motor.view().map(move |message| Message::MotorMessageContainer(i, message)))
            })
    };
    col.into()
}

fn render_device_list(devices: &[DeviceStatus]) -> Element<Message> {
    let col = Column::new()
        .spacing(TABLE_SPACING)
        .push(Text::new("Connected Devices").size(TEXT_SIZE_BIG));
    let col = if devices.is_empty() {
        col.push(Text::new("No devices"))
    } else {
        devices.iter()
            .fold(col, |column, device| {
                column.push(util::input_label(format!("{device}")))
            })
    };
    col.into()
}

async fn get_tagged_devices(application_state_db: ApplicationStateDb) -> Option<ApplicationStatus> {
    buttplug::get_tagged_devices(&application_state_db).await
}

async fn update_configuration(application_state_db: ApplicationStateDb, configuration: ConfigurationV3, warp_shutdown_tx: UnboundedSender<ShutdownMessage>) -> Result<ConfigurationV3, String> {
    crate::config::update_configuration(&application_state_db, configuration, &warp_shutdown_tx).await
}

fn tags_from_application_status(motors: &[TaggedMotor]) -> HashMap<String, MotorConfigurationV3> {
    motors.iter()
        .filter(|m| m.tag().is_some())
        .map(|m| (m.tag().unwrap().to_string(), m.motor.clone()))
        .collect()
}

fn build_example_message(motors: &[TaggedMotor]) -> String {
    motors.iter()
        .flat_map(|motor| {
            motor.tag().map(|tag| match motor.motor.feature_type {
                MotorTypeV3::Linear => format!("{tag}:20:0.5"),
                MotorTypeV3::Rotation => format!("{tag}:-0.5"),
                MotorTypeV3::Scalar { .. } => format!("{tag}:0.5"),
            })
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[inline(always)]
fn override_tag_at_index<'a>(slice: &'a [TaggedMotor], read_index: usize, override_index: usize, override_value: Option<&'a str>) -> Option<&'a str> {
    if read_index == override_index {
        override_value
    } else {
        slice[read_index].tag()
    }
}

#[inline(always)]
fn save_allowed(state: &State) -> bool {
    state.configuration_dirty && state.motor_tags_valid && !state.saving
}

#[inline(always)]
fn is_tag_valid(tag: &str) -> bool {
    !tag.contains(':') && !tag.contains(';')
}

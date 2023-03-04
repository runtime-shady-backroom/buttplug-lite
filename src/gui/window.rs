// Copyright 2022 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fmt;

use iced::{alignment::Alignment, Application, Color, Command, Element, Length, Settings, Subscription, Theme};
use iced::theme::Palette;
use iced::widget::{Button, Column, Container, Row, Rule, Scrollable, Text, TextInput};
use iced_native::Event;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};

use crate::{ApplicationStateDb, ApplicationStatus, ApplicationStatusEvent, ShutdownMessage};
use crate::configuration_v3::{ConfigurationV3, MotorConfigurationV3, MotorTypeV3};
use crate::device_status::DeviceStatus;
use crate::executor::TokioExecutor;
use crate::gui::subscription::ApplicationStatusSubscriptionProvider;

const TEXT_INPUT_PADDING: u16 = 5;
const PORT_INPUT_WIDTH: f32 = 75.0;
const TAG_INPUT_WIDTH: f32 = 100.0;
const TABLE_SPACING: u16 = 20;
const EOL_INPUT_SPACING: u16 = 5;
const TEXT_SIZE_SMALL: u16 = 12;
const TEXT_SIZE_DEFAULT: f32 = 20.0;
const TEXT_SIZE_BIG: u16 = 30;
const TEXT_SIZE_MASSIVE: u16 = 50;

const DARK_PALETTE: Palette = Palette {
    background: Color::from_rgb(
        0x36 as f32 / 255.0,
        0x39 as f32 / 255.0,
        0x3F as f32 / 255.0,
    ),
    text: Color::from_rgb(1.0, 1.0, 1.0),
    primary: Color::from_rgb(
        0x72 as f32 / 255.0,
        0x89 as f32 / 255.0,
        0xDA as f32 / 255.0,
    ),
    success: Color::from_rgb(
        0x12 as f32 / 255.0,
        0x66 as f32 / 255.0,
        0x4F as f32 / 255.0,
    ),
    danger: Color::from_rgb(
        0xC3 as f32 / 255.0,
        0x42 as f32 / 255.0,
        0x3F as f32 / 255.0,
    ),
};

lazy_static! {
    static ref THEME: Theme = Theme::custom(DARK_PALETTE);
}


pub fn run(
    application_state_db: ApplicationStateDb,
    warp_shutdown_tx: UnboundedSender<ShutdownMessage>,
    initial_devices: ApplicationStatus,
    application_status_subscription: ApplicationStatusSubscriptionProvider,
    update_url: Option<String>,
) {
    let settings = Settings {
        id: Some("buttplug-lite".to_string()),
        window: Default::default(),
        flags: Flags {
            warp_restart_tx: warp_shutdown_tx.clone(),
            application_state_db,
            initial_application_status: initial_devices,
            application_status_subscription,
            update_url,
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
    application_status_subscription: ApplicationStatusSubscriptionProvider,
    update_url: Option<String>,
}

#[derive(Debug, Clone)]
enum Message {
    SaveConfigurationRequest,
    RefreshDevices,
    RefreshDevicesComplete(Option<ApplicationStatus>),
    SaveConfigurationComplete(Result<ConfigurationV3, String>),
    PortUpdated(String),
    MotorMessage(usize, MotorMessage),
    NativeEventOccurred(Event),
    Tick,
    UpdateButtonPressed,
}

enum Gui {
    /// intermediate state used during transitions
    Loading,
    Loaded(State),
}

struct State {
    motors: Vec<TaggedMotor>,
    devices: Vec<DeviceStatus>,
    port: u16,
    port_text: String,
    warp_restart_tx: UnboundedSender<ShutdownMessage>,
    application_state_db: ApplicationStateDb,
    configuration_dirty: bool,
    saving: bool,
    last_configuration: ConfigurationV3,
    application_status_subscription: ApplicationStatusSubscriptionProvider,
    update_url: Option<String>,
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
            saving: false,
            last_configuration: configuration,
            application_status_subscription: flags.application_status_subscription,
            update_url: flags.update_url,
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
        (Gui::new(flags), Command::none())
    }

    fn title(&self) -> String {
        format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match self {
            Gui::Loading => {
                Command::none()
            }
            Gui::Loaded(state) => {
                match message {
                    Message::RefreshDevices => {
                        info!("device refresh triggered");
                        Command::perform(get_tagged_devices(state.application_state_db.clone()), Message::RefreshDevicesComplete)
                    }
                    Message::RefreshDevicesComplete(application_status) => {
                        if let Some(application_status) = application_status {
                            // we conduct the ol' switcharoo to move our old state into the new state without having to clone absolutely everything
                            if let Gui::Loaded(old_state) = std::mem::replace(self, Gui::Loading) {
                                *self = Gui::Loaded(State {
                                    devices: application_status.devices,
                                    motors: application_status.motors,
                                    port: old_state.port,
                                    port_text: old_state.port_text,
                                    warp_restart_tx: old_state.warp_restart_tx,
                                    application_state_db: old_state.application_state_db,
                                    configuration_dirty: old_state.configuration_dirty,
                                    saving: old_state.saving,
                                    last_configuration: old_state.last_configuration,
                                    application_status_subscription: old_state.application_status_subscription,
                                    update_url: old_state.update_url,
                                });
                            } else {
                                // this should never happen
                                panic!("GUI was unexpectedly not in loaded state");
                            }
                        } else {
                            panic!("Application was unexpectedly not in loaded state");
                        }

                        Command::none()
                    }
                    Message::SaveConfigurationRequest => {
                        if state.saving {
                            // this should not be possible
                            panic!("save pressed but we're already saving!");
                        } else {
                            info!("save initiated");
                            state.saving = true;

                            state.port_text = state.port.to_string();

                            // TODO: validate tags
                            let configuration = ConfigurationV3::new(state.port, tags_from_application_status(&state.motors));
                            Command::perform(update_configuration(state.application_state_db.clone(), configuration, state.warp_restart_tx.clone()), Message::SaveConfigurationComplete)
                        }
                    }
                    Message::SaveConfigurationComplete(result) => {
                        state.saving = false;
                        match result {
                            Ok(configuration) => {
                                state.last_configuration = configuration;
                                self.on_configuration_changed();
                            }
                            Err(e) => {
                                warn!("save failed: {e:?}");
                            }
                        }
                        Command::none()
                    }
                    Message::PortUpdated(new_port) => {
                        state.port_text = new_port;
                        //TODO: notify user if port is invalid
                        state.port = state.port_text.parse::<u16>().unwrap_or(state.port);
                        self.on_configuration_changed();
                        Command::none()
                    }
                    Message::MotorMessage(i, motor_message) => {
                        match state.motors.get_mut(i) {
                            Some(motor) => motor.update(motor_message),
                            None => warn!("motor index out of bounds"),
                        }
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
                        let update_url: &str = state.update_url.as_ref().expect("Somehow pressed the update button without it visible!?").as_str();
                        open::that(update_url).expect("Failed to open update URL");
                        Command::none()
                    }
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        match self {
            Gui::Loading => {
                Container::new(
                    Text::new("Loading…")
                        .size(TEXT_SIZE_MASSIVE)
                )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x()
                    .center_y()
                    .into()
            }
            Gui::Loaded(state) => {
                let example_message = format!("example message: {}", build_example_message(&state.motors));

                let save_button_text = if state.saving {
                    "saving…"
                } else {
                    "save & apply configuration"
                };
                let mut save_button = Button::new(Text::new(save_button_text));
                if state.configuration_dirty && !state.saving {
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
                            if state.update_url.is_some() {
                                row.push(
                                    Button::new(Text::new("Update Available!"))
                                        .on_press(Message::UpdateButtonPressed)
                                )
                            } else {
                                row
                            }
                        })
                        .push(Row::new()
                            .spacing(EOL_INPUT_SPACING)
                            .align_items(Alignment::Center)
                            .push(input_label("Server port:"))
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
            Gui::Loading => native_events,
        }
    }
}

/// an optionally tagged motor
#[derive(Clone, Debug)]
pub struct TaggedMotor {
    pub motor: MotorConfigurationV3,
    state: TaggedMotorState,
}

impl PartialEq for TaggedMotor {
    fn eq(&self, other: &Self) -> bool {
        (&self.motor, &self.tag()) == (&other.motor, &other.tag())
    }
}

impl Eq for TaggedMotor {}

impl PartialOrd for TaggedMotor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TaggedMotor {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.motor, &self.tag()).cmp(&(&other.motor, &other.tag()))
    }
}

#[derive(Clone, Debug)]
enum MotorMessage {
    TagUpdated(String),
    TagDeleted,
}

#[derive(Clone, Debug)]
enum TaggedMotorState {
    Tagged {
        tag: String,
    },
    Untagged,
}

impl Display for TaggedMotor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.motor, self.tag())
    }
}

impl TaggedMotor {
    pub fn new(motor: MotorConfigurationV3, tag: Option<String>) -> Self {
        let state = match tag {
            Some(tag) => TaggedMotorState::Tagged {
                tag,
            },
            None => TaggedMotorState::Untagged,
        };

        TaggedMotor {
            motor,
            state,
        }
    }

    fn tag(&self) -> Option<&str> {
        match &self.state {
            TaggedMotorState::Tagged { tag } => Some(tag),
            TaggedMotorState::Untagged => None
        }
    }

    fn update(&mut self, message: MotorMessage) {
        match message {
            MotorMessage::TagUpdated(tag) => {
                if tag.is_empty() {
                    self.state = TaggedMotorState::Untagged;
                } else {
                    self.state = TaggedMotorState::Tagged { tag };
                }
            }
            MotorMessage::TagDeleted => {
                self.state = TaggedMotorState::Untagged;
            }
        }
    }

    fn view(&self) -> Element<MotorMessage> {
        let row = Row::new()
            .spacing(EOL_INPUT_SPACING)
            .align_items(Alignment::Center)
            .push(input_label(format!("{}", &self.motor)));

        let row = match &self.state {
            TaggedMotorState::Tagged { tag  } => {
                row.push(
                    TextInput::new("motor tag", tag, MotorMessage::TagUpdated)
                        .width(Length::Fixed(TAG_INPUT_WIDTH))
                        .padding(TEXT_INPUT_PADDING)
                )
                    .push(
                        Button::new(Text::new("x")) // font doesn't support funny characters like "✕"
                            .on_press(MotorMessage::TagDeleted)
                    )
            }
            TaggedMotorState::Untagged => {
                row.push(
                    TextInput::new("motor tag", "", MotorMessage::TagUpdated)
                        .width(Length::Fixed(TAG_INPUT_WIDTH))
                        .padding(TEXT_INPUT_PADDING)
                )
            }
        };

        row.into()
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
                column.push(motor.view().map(move |message| Message::MotorMessage(i, message)))
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
                column.push(input_label(format!("{device}")))
            })
    };
    col.into()
}

async fn get_tagged_devices(application_state_db: ApplicationStateDb) -> Option<ApplicationStatus> {
    crate::get_tagged_devices(&application_state_db).await
}

async fn update_configuration(application_state_db: ApplicationStateDb, configuration: ConfigurationV3, warp_shutdown_tx: UnboundedSender<ShutdownMessage>) -> Result<ConfigurationV3, String> {
    crate::update_configuration(&application_state_db, configuration, &warp_shutdown_tx).await
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
                MotorTypeV3::Scalar { actuator_type: _ } => format!("{tag}:0.5"),
            })
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn input_label<'a, S: Into<Cow<'a, str>>, T: 'a>(label: S) -> Element<'a, T> {
    let text = Text::new(label);

    Container::new(text)
        .padding(TEXT_INPUT_PADDING)
        .into()
}

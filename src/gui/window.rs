use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fmt;

use iced::{Align, Application, button, Button, Column, Command, Container, Element, Length, Row, Rule, scrollable, Scrollable, Settings, Text, text_input, TextInput, Clipboard, Subscription};
use iced_native::{window, Event};
use tokio::sync::mpsc::UnboundedSender;

use crate::{ApplicationStateDb, ApplicationStatus, DeviceStatus, ShutdownMessage};
use crate::configuration::{Configuration, Motor};
use crate::executor::TokioExecutor;

use super::theme::Theme;

const TEXT_INPUT_PADDING: u16 = 5;
const PORT_INPUT_WIDTH: u16 = 75;
const TAG_INPUT_WIDTH: u16 = 100;
const TABLE_SPACING: u16 = 20;
const EOL_INPUT_SPACING: u16 = 5;
const TEXT_SIZE_SMALL: u16 = 12;
const TEXT_SIZE_DEFAULT: u16 = 20;
const TEXT_SIZE_BIG: u16 = 30;
const TEXT_SIZE_MASSIVE: u16 = 50;
const STYLE: Theme = Theme::Dark;

pub fn run(
    application_state_db: ApplicationStateDb,
    warp_shutdown_tx: UnboundedSender<ShutdownMessage>,
    initial_devices: ApplicationStatus,
) {
    let settings = Settings {
        window: Default::default(),
        flags: Flags {
            warp_restart_tx: warp_shutdown_tx.clone(),
            application_state_db,
            initial_devices,
        },
        default_font: Default::default(),
        default_text_size: TEXT_SIZE_DEFAULT,
        antialiasing: true,
        exit_on_close_request: false,
    };

    Gui::run(settings).expect("could not instantiate window");
    match warp_shutdown_tx.send(ShutdownMessage::Shutdown) {
        Ok(()) => println!("shutdown triggered by UI close"),
        Err(e) => panic!("Error triggering shutdown: {}", e)
    };
}

struct Flags {
    warp_restart_tx: UnboundedSender<ShutdownMessage>,
    application_state_db: ApplicationStateDb,
    initial_devices: ApplicationStatus,
}

#[derive(Debug, Clone)]
enum Message {
    SaveConfigurationRequest,
    RefreshDevicesRequest,
    RefreshDevicesComplete(ApplicationStatus),
    SaveConfigurationComplete(Result<(), String>),
    PortUpdated(String),
    MotorMessage(usize, MotorMessage),
    EventOccurred(iced_native::Event),
}

enum Gui {
    /// intermediate state used during transitions
    Loading,
    Loaded(State),
}

struct State {
    devices: ApplicationStatus,
    scroll: scrollable::State,
    port: u16,
    port_text: String,
    port_input: text_input::State,
    save_configuration_button: button::State,
    refresh_devices_button: button::State,
    restart_warp_button: button::State,
    warp_restart_tx: UnboundedSender<ShutdownMessage>,
    application_state_db: ApplicationStateDb,
    should_exit: bool,
}

impl Gui {
    fn new(flags: Flags) -> Self {
        let port = flags.initial_devices.port;
        Gui::Loaded(State {
            devices: flags.initial_devices,
            scroll: Default::default(),
            port,
            port_text: port.to_string(),
            port_input: Default::default(),
            save_configuration_button: Default::default(),
            refresh_devices_button: Default::default(),
            restart_warp_button: Default::default(),
            warp_restart_tx: flags.warp_restart_tx,
            application_state_db: flags.application_state_db,
            should_exit: false,
        })
    }
}

impl Application for Gui {
    type Executor = TokioExecutor;
    type Message = Message;
    type Flags = Flags;

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (Gui::new(flags), Command::none())
    }

    fn title(&self) -> String {
        format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")).into()
    }

    fn update(&mut self, message: Self::Message, _clipboard: &mut Clipboard) -> Command<Self::Message> {
        match self {
            Gui::Loading => {
                Command::none()
            }
            Gui::Loaded(state) => {
                match message {
                    Message::RefreshDevicesRequest => {
                        println!("refresh pressed");
                        Command::perform(get_tagged_devices(state.application_state_db.clone()), Message::RefreshDevicesComplete)
                    }
                    Message::RefreshDevicesComplete(devices) => {
                        // we conduct the ol' switcharoo to move our old state into the new state without having to clone absolutely everything
                        if let Gui::Loaded(old_state) = std::mem::replace(self, Gui::Loading) {
                            *self = Gui::Loaded(State {
                                devices, // the only thing that is updated
                                scroll: old_state.scroll,
                                port: old_state.port,
                                port_text: old_state.port_text,
                                port_input: old_state.port_input,
                                save_configuration_button: old_state.save_configuration_button,
                                refresh_devices_button: old_state.refresh_devices_button,
                                restart_warp_button: old_state.restart_warp_button,
                                warp_restart_tx: old_state.warp_restart_tx,
                                application_state_db: old_state.application_state_db,
                                should_exit: old_state.should_exit,
                            });
                        } else {
                            // this should never happen
                            panic!("GUI was unexpectedly not in loaded state");
                        }

                        Command::none()
                    }
                    Message::SaveConfigurationRequest => {
                        println!("save pressed");

                        //TODO: notify user if port is invalid
                        state.port = state.port_text.parse::<u16>().unwrap_or(state.port);
                        state.port_text = state.port.to_string();

                        let configuration = Configuration {
                            port: state.port,
                            tags: tags_from_application_status(&state.devices),
                        };

                        Command::perform(update_configuration(state.application_state_db.clone(), configuration, state.warp_restart_tx.clone()), Message::SaveConfigurationComplete)
                    }
                    Message::SaveConfigurationComplete(result) => {
                        println!("save completed: {:?}", result);
                        Command::none()
                    }
                    Message::PortUpdated(new_port) => {
                        state.port_text = new_port;
                        Command::none()
                    }
                    Message::MotorMessage(i, motor_message) => {
                        match state.devices.motors.get_mut(i) {
                            Some(motor) => motor.update(motor_message),
                            None => eprintln!("motor index out of bounds"),
                        }
                        Command::none()
                    }
                    Message::EventOccurred(event) => {
                        if let Event::Window(window::Event::CloseRequested) = event {
                            println!("received gui shutdown request"); //TODO: actually run shutdown code
                            state.should_exit = true;
                        }
                        Command::none()
                    }
                }
            }
        }
    }

    fn view(&mut self) -> Element<'_, Self::Message> {
        match self {
            Gui::Loading => {
                Container::new(
                    Text::new("Loading…")
                        .size(TEXT_SIZE_MASSIVE)
                )
                    .style(STYLE)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x()
                    .center_y()
                    .into()
            }
            Gui::Loaded(state) => {
                let example_message = format!("example message: {}", build_example_message(&state.devices.motors));

                let content = Scrollable::new(&mut state.scroll)
                    .style(STYLE)
                    .padding(TABLE_SPACING)
                    .push(Column::new()
                        .spacing(TABLE_SPACING)
                        .width(Length::Fill)
                        .push(Row::new()
                            .spacing(TABLE_SPACING)
                            .push(
                                Button::new(&mut state.save_configuration_button, Text::new("refresh devices"))
                                    .style(STYLE)
                                    .on_press(Message::RefreshDevicesRequest)
                            )
                            .push(
                                Button::new(&mut state.refresh_devices_button, Text::new("apply configuration"))
                                    .style(STYLE)
                                    .on_press(Message::SaveConfigurationRequest)
                            )
                        )
                        .push(Row::new()
                            .spacing(EOL_INPUT_SPACING)
                            .align_items(Align::Center)
                            .push(input_label("Server port:3031"))
                            .push(
                                TextInput::new(&mut state.port_input, "server port", state.port_text.as_str(), Message::PortUpdated)
                                    .style(STYLE)
                                    .width(Length::Units(PORT_INPUT_WIDTH))
                                    .padding(TEXT_INPUT_PADDING)
                            )
                        )
                        .push(
                            Rule::horizontal(TABLE_SPACING)
                                .style(STYLE)
                        )
                        .push(Row::new()
                            .spacing(TABLE_SPACING)
                            .push(
                                render_motor_list(&mut state.devices.motors)
                            )
                            .push(
                                render_device_list(&state.devices.devices)
                            )
                        )
                        .push(
                            Rule::horizontal(TABLE_SPACING)
                                .style(STYLE)
                        )
                        .push(Text::new(example_message).size(TEXT_SIZE_SMALL))
                    );

                Container::new(content)
                    .style(STYLE)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        iced_native::subscription::events().map(Message::EventOccurred)
    }

    fn should_exit(&self) -> bool {
        match self {
            Gui::Loading => {
                false
            }
            Gui::Loaded(state) => {
                state.should_exit
            }
        }
    }
}

/// an optionally tagged motor
#[derive(Clone, Debug)]
pub struct TaggedMotor {
    pub motor: Motor,
    tag_text: text_input::State,
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
        delete_tag_button: button::State,
    },
    Untagged,
}

impl Display for TaggedMotor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.motor, self.tag())
    }
}

impl TaggedMotor {
    pub fn new(motor: Motor, tag: Option<String>) -> Self {
        let state = match tag {
            Some(tag) => TaggedMotorState::Tagged {
                tag,
                delete_tag_button: Default::default(),
            },
            None => TaggedMotorState::Untagged,
        };

        TaggedMotor {
            motor,
            tag_text: Default::default(),
            state,
        }
    }

    fn tag(&self) -> Option<&str> {
        match &self.state {
            TaggedMotorState::Tagged { tag, delete_tag_button: _ } => Some(tag),
            TaggedMotorState::Untagged => None
        }
    }

    fn update(&mut self, message: MotorMessage) {
        match message {
            MotorMessage::TagUpdated(tag) => {
                if tag.is_empty() {
                    self.state = TaggedMotorState::Untagged;
                } else {
                    self.state = match self.state {
                        TaggedMotorState::Tagged { tag: _, delete_tag_button } => TaggedMotorState::Tagged {
                            tag,
                            delete_tag_button,
                        },
                        TaggedMotorState::Untagged => TaggedMotorState::Tagged {
                            tag,
                            delete_tag_button: Default::default(),
                        },
                    };
                }
            }
            MotorMessage::TagDeleted => {
                self.state = TaggedMotorState::Untagged;
            }
        }
    }

    fn view(&mut self) -> Element<MotorMessage> {
        let row = Row::new()
            .spacing(EOL_INPUT_SPACING)
            .align_items(Align::Center)
            .push(input_label(format!("{}", &self.motor)));

        let row = match &mut self.state {
            TaggedMotorState::Tagged { tag, delete_tag_button } => {
                row.push(
                    TextInput::new(&mut self.tag_text, "motor tag", tag, MotorMessage::TagUpdated)
                        .style(STYLE)
                        .width(Length::Units(TAG_INPUT_WIDTH))
                        .padding(TEXT_INPUT_PADDING)
                )
                    .push(
                        Button::new(delete_tag_button, Text::new("x"))
                            .style(STYLE)
                            .on_press(MotorMessage::TagDeleted)
                    )
            }
            TaggedMotorState::Untagged => {
                row.push(
                    TextInput::new(&mut self.tag_text, "motor tag", "", MotorMessage::TagUpdated)
                        .style(STYLE)
                        .width(Length::Units(TAG_INPUT_WIDTH))
                        .padding(TEXT_INPUT_PADDING)
                )
            }
        };

        row.into()
    }
}

fn render_motor_list(motors: &mut Vec<TaggedMotor>) -> Element<Message> {
    let col = Column::new()
        .spacing(TABLE_SPACING)
        .push(Text::new("Motors").size(TEXT_SIZE_BIG));
    let col = if motors.is_empty() {
        col.push(Text::new("No motors"))
    } else {
        motors.iter_mut()
            .enumerate()
            .fold(col, |column, (i, motor)| {
                column.push(motor.view().map(move |message| Message::MotorMessage(i, message)))
            })
    };
    col.into()
}

fn render_device_list(devices: &Vec<DeviceStatus>) -> Element<Message> {
    let col = Column::new()
        .spacing(TABLE_SPACING)
        .push(Text::new("Devices").size(TEXT_SIZE_BIG));
    let col = if devices.is_empty() {
        col.push(Text::new("No devices"))
    } else {
        devices.iter()
            .fold(col, |column, device| {
                column.push(input_label(format!("{}", device)))
            })
    };
    col.into()
}

async fn get_tagged_devices(application_state_db: ApplicationStateDb) -> ApplicationStatus {
    crate::get_tagged_devices(&application_state_db).await
}

async fn update_configuration(application_state_db: ApplicationStateDb, configuration: Configuration, warp_shutdown_tx: UnboundedSender<ShutdownMessage>) -> Result<(), String> {
    crate::update_configuration(&application_state_db, configuration, &warp_shutdown_tx).await
}

fn tags_from_application_status(application_status: &ApplicationStatus) -> HashMap<String, Motor> {
    application_status.motors.iter()
        .filter(|m| m.tag().is_some())
        .map(|m| (m.tag().unwrap().to_string(), m.motor.clone()))
        .collect()
}

fn build_example_message(motors: &Vec<TaggedMotor>) -> String {
    motors.iter()
        .flat_map(|motor| motor.tag())
        .map(|tag| format!("{}:0.5", tag))
        .collect::<Vec<_>>()
        .join(";")
}

fn input_label<'a, S: Into<String>, T: 'a>(label: S) -> Element<'a, T> {
    let text = Text::new(label);

    Container::new(text)
        .padding(TEXT_INPUT_PADDING)
        .style(STYLE)
        .into()
}

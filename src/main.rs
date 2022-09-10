#[macro_use]
extern crate lazy_static;

use std::{convert, fs};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::ops::DerefMut;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::time::Duration;

use buttplug::client::{ButtplugClient, ButtplugClientDevice, ButtplugClientEvent, device::LinearCommand, device::RotateCommand, device::VibrateCommand, ScalarCommand};
use buttplug::core::connector::ButtplugInProcessClientConnectorBuilder;
use buttplug::core::message::ButtplugDeviceMessageType;
use buttplug::server::ButtplugServerBuilder;
use buttplug::server::device::hardware::communication::btleplug::BtlePlugCommunicationManagerBuilder;
use buttplug::server::device::hardware::communication::lovense_connect_service::LovenseConnectServiceCommunicationManagerBuilder;
use buttplug::server::device::hardware::communication::lovense_dongle::{LovenseHIDDongleCommunicationManagerBuilder, LovenseSerialDongleCommunicationManagerBuilder};
use buttplug::server::device::hardware::communication::serialport::SerialPortCommunicationManagerBuilder;
use clap::Parser;
use directories::ProjectDirs;
use futures::StreamExt;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot::Sender;
use tokio::task;
use tracing::Level;
use tracing_subscriber::util::SubscriberInitExt;
use warp::Filter;

use cli_args::CliArgs;
use configuration_minimal::ConfigurationMinimal;
use configuration_v2::ConfigurationV2;
use configuration_v3::{ConfigurationV3, MotorConfigurationV3, MotorTypeV3};
use device_status::DeviceStatus;
use gui::subscription::{ApplicationStatusEvent, ApplicationStatusSubscriptionProvider};
use gui::window::TaggedMotor;
use motor_settings::MotorSettings;
use watchdog::WatchdogTimeoutDb;

mod configuration_v2;
mod watchdog;
mod gui;
mod executor;
mod motor_settings;
mod device_status;
mod cli_args;
mod configuration_v3;
mod configuration_minimal;


pub const CONFIG_VERSION: i32 = 3;

// global state types
pub type ApplicationStateDb = Arc<RwLock<Option<ApplicationState>>>;

// how long to wait before attempting a reconnect to the server
const BUTTPLUG_SERVER_RECONNECT_DELAY_MILLIS: u64 = 5000;

// name of this client from the buttplug.io server's perspective
static BUTTPLUG_CLIENT_NAME: &str = "in-process-client";

// log prefixes:
static LOG_PREFIX_HAPTIC_ENDPOINT: &str = "/haptic";
static LOG_PREFIX_BUTTPLUG_SERVER: &str = "buttplug_server";

static CONFIG_FILE_NAME: &str = "config.toml";

lazy_static! {
    static ref CONFIG_DIR_FILE_PATH: PathBuf = create_config_file_path();
    pub static ref TOKIO_RUNTIME: tokio::runtime::Runtime = create_tokio_runtime();
}

// eventually I'd like some way to get a ref to the server in here
pub struct ApplicationState {
    pub client: ButtplugClient,
    pub configuration: ConfigurationV3,
}

#[derive(Debug)]
pub enum ShutdownMessage {
    Restart,
    Shutdown,
}

fn main() {
    TOKIO_RUNTIME.block_on(tokio_main())
}

async fn tokio_main() {
    let args: CliArgs = CliArgs::parse();

    println!("initializing {} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    if args.verbose > 0 {
        let level = if args.verbose > 1 {
            Level::DEBUG
        } else {
            Level::INFO
        };
        tracing_subscriber::fmt()
            .with_max_level(level)
            .finish()
            .init();
    }

    let watchdog_timeout_db: WatchdogTimeoutDb = Arc::new(AtomicI64::new(i64::MAX));
    let application_state_db: ApplicationStateDb = Arc::new(RwLock::new(None));

    // GET / => 200 OK with body application name and version
    let info = warp::path::end()
        .and(warp::get())
        .map(|| format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")));

    // GET /hapticstatus => 200 OK with body containing haptic status
    let hapticstatus = warp::path("hapticstatus")
        .and(warp::get())
        .and(with_db(application_state_db.clone()))
        .and_then(haptic_status_handler);

    // GET /batterystatus => list of battery levels, spaced with newlines
    let batterystatus = warp::path("batterystatus")
        .and(warp::get())
        .and(with_db(application_state_db.clone()))
        .and_then(battery_status_handler);

    // GET /batterystatus => list of battery levels, spaced with newlines
    let deviceconfig = warp::path("deviceconfig")
        .and(warp::get())
        .and(with_db(application_state_db.clone()))
        .and_then(device_config_handler);

    // WEBSOCKET /haptic
    let haptic = warp::path("haptic")
        .and(warp::ws())
        .and(with_db(application_state_db.clone()))
        .and(with_db(watchdog_timeout_db.clone()))
        .map(|ws: warp::ws::Ws, application_state_db: ApplicationStateDb, haptic_watchdog_db: WatchdogTimeoutDb| {
            ws.on_upgrade(|ws| haptic_handler(ws, application_state_db, haptic_watchdog_db))
        });

    let routes = info
        .or(hapticstatus)
        .or(batterystatus)
        .or(deviceconfig)
        .or(haptic);

    watchdog::start(watchdog_timeout_db, application_state_db.clone());

    // used to send initial port over from the configuration load
    let (initial_config_loaded_tx, initial_config_loaded_rx) = oneshot::channel::<()>();
    let mut initial_config_loaded_tx = Some(initial_config_loaded_tx);
    let (application_status_sender, application_status_receiver) = mpsc::unbounded_channel::<ApplicationStatusEvent>();

    // test ticks
    let test_tick_sender = application_status_sender.clone();
    task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(10000));
        loop {
            interval.tick().await;
            test_tick_sender.send(ApplicationStatusEvent::next_tick()).expect("WHO DROPPED MY FREAKING RECEIVER?");
        }
    });

    // connector clone moved into reconnect task
    let reconnector_application_state_clone = application_state_db.clone();

    // spawn the server reconnect task
    // when the server is connected this functions as the event reader
    // when the server is disconnected it attempts to reconnect after a delay
    task::spawn(async move {
        loop {
            // we reconnect here regardless of server state
            start_buttplug_server(reconnector_application_state_clone.clone(), initial_config_loaded_tx, application_status_sender.clone()).await; // will "block" until disconnect
            initial_config_loaded_tx = None; // only Some() for the first loop
            tokio::time::sleep(Duration::from_millis(BUTTPLUG_SERVER_RECONNECT_DELAY_MILLIS)).await; // reconnect delay
        }
    });

    let (warp_shutdown_initiate_tx, mut warp_shutdown_initiate_rx) = mpsc::unbounded_channel::<ShutdownMessage>();

    // called once warp is done dying
    let (warp_shutdown_complete_tx, warp_shutdown_complete_rx) = oneshot::channel::<()>();

    // triggers the GUI to start, only called after warp spins up
    let (gui_start_tx, gui_start_rx) = oneshot::channel::<()>();
    let mut gui_start_oneshot_tx = Some(gui_start_tx); // will get None'd after the first loop

    // moved into the following task
    let reconnect_task_application_state_db_clone = application_state_db.clone();

    task::spawn(async move {
        initial_config_loaded_rx.await.expect("failed to load initial configuration");

        // loop handles restarting the warp server if needed
        loop {
            // used to proxy the signal from the mpsc into the graceful_shutdown closure later
            // this is needed because we cannot move the mpsc consumer
            let (warp_shutdown_oneshot_tx, warp_shutdown_oneshot_rx) = oneshot::channel::<()>();

            let port = reconnect_task_application_state_db_clone.read().await.as_ref().expect("failed to read initial configuration").configuration.port;
            let proxy_server_address: SocketAddr = ([127, 0, 0, 1], port).into();

            let server = warp::serve(routes.clone())
                .try_bind_with_graceful_shutdown(proxy_server_address, async move {
                    warp_shutdown_oneshot_rx.await.expect("error receiving warp shutdown signal");
                    println!("shutting down web server")
                });

            let shutdown_message = match server {
                Ok((address, warp_future)) => {
                    println!("starting web server on {}", address);

                    // only start the GUI once we've successfully started the web server in the first loop iteration
                    if let Some(sender) = gui_start_oneshot_tx {
                        sender.send(()).expect("error transmitting gui startup signal");
                        gui_start_oneshot_tx = None;
                    }

                    // run warp in the background
                    task::spawn(async move {
                        warp_future.await;
                    });

                    // sacrifice main thread to shutdown trigger bullshit
                    let signal = warp_shutdown_initiate_rx.recv().await.unwrap_or(ShutdownMessage::Shutdown);
                    warp_shutdown_oneshot_tx.send(()).expect("error transmitting warp shutdown signal");
                    signal
                }
                Err(e) => {
                    //TODO: what happens if the default port is used? The user needs some way to change it.
                    eprintln!("Failed to start web server: {:?}", e);
                    ShutdownMessage::Shutdown
                }
            };

            if let ShutdownMessage::Shutdown = shutdown_message {
                break;
            }
            // otherwise we go again
        }
        warp_shutdown_complete_tx.send(()).expect("warp shut down, but could not transmit callback signal");
    });

    if let Ok(()) = gui_start_rx.await {
        //TODO: wait for buttplug to notice devices
        let initial_devices = get_tagged_devices(&application_state_db).await.expect("Application failed to initialize");

        let subscription = ApplicationStatusSubscriptionProvider::new(application_status_receiver);
        gui::window::run(application_state_db, warp_shutdown_initiate_tx, initial_devices, subscription); // blocking call

        // NOTE: iced hard kills the application when the windows is closed!
        // That means this code is unreachable.
        // As far as I'm aware it is currently impossible to register any sort of shutdown
        // hook/return/signal from iced once you sacrifice your main thread.
        // For now this is fine, as we don't have any super sensitive shutdown code to run,
        // especially with the buttplug server being (apparently?) unstoppable.
    }

    // at this point we begin cleaning up resources for shutdown
    println!("shutting down...");

    // but first, wait for warp to close
    if let Err(e) = warp_shutdown_complete_rx.await {
        eprintln!("error shutting down warp: {:?}", e)
    }
}

fn create_config_file_path() -> PathBuf {
    let config_dir_path: PathBuf = ProjectDirs::from("io.github", "runtime-shady-backroom", env!("CARGO_PKG_NAME"))
        .expect("unable to locate configuration directory")
        .config_dir()
        .into();
    fs::create_dir_all(config_dir_path.as_path()).expect("failed to create configuration directory");
    config_dir_path.join(CONFIG_FILE_NAME)
}

fn create_tokio_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime")
}

// start server, then while running process events
// returns only when we disconnect from the server
async fn start_buttplug_server(
    application_state_db: ApplicationStateDb,
    initial_config_loaded_tx: Option<Sender<()>>,
    application_status_event_sender: UnboundedSender<ApplicationStatusEvent>,
) {
    let mut application_state_mutex = application_state_db.write().await;
    let buttplug_client = ButtplugClient::new(BUTTPLUG_CLIENT_NAME);

    let mut server_builder = ButtplugServerBuilder::default();
    server_builder
        .name("buttplug-lite")
        .comm_manager(BtlePlugCommunicationManagerBuilder::default())
        .comm_manager(SerialPortCommunicationManagerBuilder::default())
        .comm_manager(LovenseHIDDongleCommunicationManagerBuilder::default())
        .comm_manager(LovenseSerialDongleCommunicationManagerBuilder::default())
        .comm_manager(LovenseConnectServiceCommunicationManagerBuilder::default());

    #[cfg(target_os = "windows")] {
        use buttplug::server::device::hardware::communication::xinput::XInputDeviceCommunicationManagerBuilder;
        server_builder.comm_manager(XInputDeviceCommunicationManagerBuilder::default());
    }

    let server = server_builder
        .finish()
        .expect("Failed to initialize buttplug server");

    let connector = ButtplugInProcessClientConnectorBuilder::default()
        .server(server)
        .finish();

    match buttplug_client.connect(connector).await {
        Ok(()) => {
            println!("{}: Device server started!", LOG_PREFIX_BUTTPLUG_SERVER);
            let mut event_stream = buttplug_client.event_stream();
            match buttplug_client.start_scanning().await {
                Ok(()) => println!("{}: starting device scan", LOG_PREFIX_BUTTPLUG_SERVER),
                Err(e) => eprintln!("{}: scan failure: {:?}", LOG_PREFIX_BUTTPLUG_SERVER, e)
            };

            // reuse old config, or load from disk if this is the initial connection
            let previous_state = application_state_mutex.deref_mut().take();
            let configuration = match previous_state {
                Some(ApplicationState { configuration, client: _ }) => configuration,
                None => {
                    println!("{}: Attempting to load config from {:?}", LOG_PREFIX_BUTTPLUG_SERVER, *CONFIG_DIR_FILE_PATH);
                    let loaded_configuration: Result<ConfigurationMinimal, String> = fs::read_to_string(CONFIG_DIR_FILE_PATH.as_path())
                        .map_err(|e| format!("{:?}", e))
                        .and_then(|string| toml::from_str(&string).map_err(|e| format!("{:?}", e)));
                    let configuration: ConfigurationV3 = match loaded_configuration {
                        Ok(configuration) => {
                            let loaded_configuration: Result<ConfigurationV3, String> = if configuration.version < 3 {
                                println!("converting v{} config to v{}", configuration.version, CONFIG_VERSION);
                                fs::read_to_string(CONFIG_DIR_FILE_PATH.as_path())
                                    .map_err(|e| format!("{:?}", e))
                                    .and_then(|string| toml::from_str::<ConfigurationV2>(&string).map_err(|e| format!("{:?}", e)))
                                    .map(|config| config.into())
                            } else {
                                fs::read_to_string(CONFIG_DIR_FILE_PATH.as_path())
                                    .map_err(|e| format!("{:?}", e))
                                    .and_then(|string| toml::from_str::<ConfigurationV3>(&string).map_err(|e| format!("{:?}", e)))
                            };

                            match loaded_configuration {
                                Ok(configuration) => configuration,
                                Err(e) => {
                                    //TODO: attempt to backup old config file when read fails
                                    eprintln!("falling back to default config due to error: {}", e);
                                    ConfigurationV3::default()
                                }
                            }
                        }
                        Err(e) => {
                            //TODO: attempt to backup old config file when read fails
                            eprintln!("falling back to default config due to error: {}", e);
                            ConfigurationV3::default()
                        }
                    };
                    println!("{}: Loaded configuration v{} from disk", LOG_PREFIX_BUTTPLUG_SERVER, configuration.version);

                    if configuration.is_outdated() {
                        let new_configuration = configuration.new_with_current_version();
                        match save_configuration(&new_configuration).await {
                            Ok(_) => {
                                println!("{}: Migrated configuration to new directory", LOG_PREFIX_BUTTPLUG_SERVER);
                                new_configuration
                            }
                            Err(e) => {
                                eprintln!("{}: Error migrating configuration to new directory: {}", LOG_PREFIX_BUTTPLUG_SERVER, e);
                                configuration
                            }
                        }
                    } else {
                        configuration
                    }
                }
            };

            *application_state_mutex = Some(ApplicationState { client: buttplug_client, configuration });
            drop(application_state_mutex); // prevent this section from requiring two

            if let Some(sender) = initial_config_loaded_tx {
                sender.send(()).expect("failed to send config-loaded signal");
            }

            loop {
                match event_stream.next().await {
                    Some(event) => match event {
                        ButtplugClientEvent::DeviceAdded(dev) => {
                            println!("{}: device connected: {}", LOG_PREFIX_BUTTPLUG_SERVER, dev.name());
                            application_status_event_sender.send(ApplicationStatusEvent::DeviceAdded).expect("failed to send device added event");
                        }
                        ButtplugClientEvent::DeviceRemoved(dev) => {
                            println!("{}: device disconnected: {}", LOG_PREFIX_BUTTPLUG_SERVER, dev.name());
                            application_status_event_sender.send(ApplicationStatusEvent::DeviceRemoved).expect("failed to send device removed event");
                        }
                        ButtplugClientEvent::PingTimeout => println!("{}: ping timeout", LOG_PREFIX_BUTTPLUG_SERVER),
                        ButtplugClientEvent::Error(e) => println!("{}: server error: {:?}", LOG_PREFIX_BUTTPLUG_SERVER, e),
                        ButtplugClientEvent::ScanningFinished => println!("{}: device scan finished", LOG_PREFIX_BUTTPLUG_SERVER),
                        ButtplugClientEvent::ServerConnect => println!("{}: server connected", LOG_PREFIX_BUTTPLUG_SERVER),
                        ButtplugClientEvent::ServerDisconnect => {
                            println!("{}: server disconnected", LOG_PREFIX_BUTTPLUG_SERVER);
                            let mut application_state_mutex = application_state_db.write().await;
                            *application_state_mutex = None; // not strictly required but will give more sane error messages
                            break;
                        }
                    },
                    None => eprintln!("{}: error reading haptic event", LOG_PREFIX_BUTTPLUG_SERVER)
                };
            }
        }
        Err(e) => eprintln!("{}: failed to connect to server. Will retry shortly... ({:?})", LOG_PREFIX_BUTTPLUG_SERVER, e) // will try to reconnect later, may not need to log this error
    }
}

fn with_db<T: Clone + Send>(db: T) -> impl Filter<Extract=(T, ), Error=convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

pub async fn update_configuration(application_state_db: &ApplicationStateDb, configuration: ConfigurationV3, warp_shutdown_tx: &UnboundedSender<ShutdownMessage>) -> Result<ConfigurationV3, String> {
    save_configuration(&configuration).await?;
    let mut lock = application_state_db.write().await;
    let previous_state = lock.deref_mut().take();
    match previous_state {
        Some(ApplicationState { client, configuration: previous_configuration }) => {
            let new_port = configuration.port;
            *lock = Some(ApplicationState {
                client,
                configuration: configuration.clone(),
            });
            drop(lock);

            // restart warp if necessary
            if new_port != previous_configuration.port {
                warp_shutdown_tx.send(ShutdownMessage::Restart)
                    .map_err(|e| format!("{:?}", e))?;
            }

            Ok(configuration)
        }
        None => Err("cannot update configuration until after initial haptic server startup".into())
    }
}

async fn save_configuration(configuration: &ConfigurationV3) -> Result<(), String> {
    // config serialization should never fail, so we should be good to panic
    let serialized_config = toml::to_string(configuration).expect("failed to serialize configuration");
    task::spawn_blocking(|| {
        fs::write(CONFIG_DIR_FILE_PATH.as_path(), serialized_config).map_err(|e| format!("{:?}", e))
    }).await
        .map_err(|e| format!("{:?}", e))
        .and_then(convert::identity)
}

/// full list of all device information we could ever want
#[derive(Clone, Debug)]
pub struct ApplicationStatus {
    pub motors: Vec<TaggedMotor>,
    pub devices: Vec<DeviceStatus>,
    pub configuration: ConfigurationV3,
}

pub async fn get_tagged_devices(application_state_db: &ApplicationStateDb) -> Option<ApplicationStatus> {
    let application_state_mutex = application_state_db.read().await;
    match application_state_mutex.as_ref() {
        Some(application_state) => {
            let DeviceList { motors, mut devices } = get_devices(application_state).await;
            let configuration = &application_state.configuration;
            let tags = &configuration.tags;

            // convert tags to TaggedMotor
            let mut tagged_motors = motors_to_tagged(tags);

            // for each device not yet in TaggedMotor, generate a new dummy TaggedMotor
            let mut missing_motors: Vec<TaggedMotor> = motors.into_iter()
                .filter(|motor| !tagged_motors.iter().any(|possible_match| &possible_match.motor == motor))
                .map(|missing_motor| TaggedMotor::new(missing_motor, None))
                .collect();

            // merge results
            tagged_motors.append(&mut missing_motors);

            // sort the things
            tagged_motors.sort_unstable();
            devices.sort_unstable();

            Some(ApplicationStatus {
                motors: tagged_motors,
                devices,
                configuration: configuration.clone(),
            })
        }
        None => None
    }
}

fn motors_to_tagged(tags: &HashMap<String, MotorConfigurationV3>) -> Vec<TaggedMotor> {
    tags.iter()
        .map(|(tag, motor)| TaggedMotor::new(motor.clone(), Some(tag.clone())))
        .collect()
}

/// intermediate struct used to return partially processed device info
struct DeviceList {
    motors: Vec<MotorConfigurationV3>,
    devices: Vec<DeviceStatus>,
}

async fn get_devices(application_state: &ApplicationState) -> DeviceList {
    let devices = application_state.client.devices();
    let mut device_statuses: Vec<DeviceStatus> = Vec::with_capacity(devices.len());

    for device in devices.iter() {
        let battery_level = if device.message_attributes().message_allowed(&ButtplugDeviceMessageType::BatteryLevelCmd) {
            device.battery_level().await.ok()
        } else {
            None
        };
        let rssi_level = if device.message_attributes().message_allowed(&ButtplugDeviceMessageType::RSSILevelCmd) {
            device.rssi_level().await.ok()
        } else {
            None
        };
        let name: String = device.name().to_string();
        device_statuses.push(DeviceStatus { name, battery_level, rssi_level })
    }

    let motors = Vec::new();
    //TODO:
    // let motors = devices.into_iter()
    //     .flat_map(|device| {
    //         MotorTypeV3::iter()
    //             .flat_map(move |feature_type| {
    //                 let device_name = device.name().clone();
    //
    //                 let feature_count = device_feature_count_by_type(feature_type, &device);
    //                 let feature_range = 0..feature_count;
    //                 feature_range.into_iter()
    //                     .map(move |feature_index| {
    //                         MotorConfigurationV3 {
    //                             device_name: device_name.to_string(),
    //                             feature_index,
    //                             feature_type: feature_type.clone(),
    //                         }
    //                     })
    //             })
    //     })
    //     .collect();

    DeviceList {
        motors,
        devices: device_statuses,
    }
}

fn device_feature_count_by_type(device_type: &MotorTypeV3, device: &ButtplugClientDevice) -> u32 {
    match device_type.get_type() {
        Some(message_type) => {
            //TODO:
            // device.allowed_messages.get(&message_type)
            //     .and_then(|attributes| attributes.feature_count)
            //     .unwrap_or(0)
            0
        }
        None => {
            match device_type {
                MotorTypeV3::Linear => panic!("linear type should have already been handled"),
                MotorTypeV3::Rotation => panic!("rotation type should have already been handled"),
                MotorTypeV3::Scalar(_) => panic!("scalar type should have already been handled"),
            }
        }
    }
}

// return a device status summary
async fn haptic_status_handler(application_state_db: ApplicationStateDb) -> Result<impl warp::Reply, warp::Rejection> {
    let application_state_mutex = application_state_db.read().await;
    match application_state_mutex.as_ref() {
        Some(application_state) => {
            let connected = application_state.client.connected();
            let mut string = format!("device server running={}", connected);
            for device in application_state.client.devices() {
                string.push_str(format!("\n  {}", device.name()).as_str());

                let scalar_cmds = device.message_attributes().scalar_cmd().iter()
                    .flat_map(|inner| inner.iter())
                    .map(|value| (ButtplugDeviceMessageType::ScalarCmd, value));

                let rotate_cmds = device.message_attributes().rotate_cmd().iter()
                    .flat_map(|inner| inner.iter())
                    .map(|value| (ButtplugDeviceMessageType::RotateCmd, value));

                let linear_cmds = device.message_attributes().linear_cmd().iter()
                    .flat_map(|inner| inner.iter())
                    .map(|value| (ButtplugDeviceMessageType::LinearCmd, value));

                let attributes = scalar_cmds.chain(rotate_cmds).chain(linear_cmds);

                for (message_type, attributes) in attributes {
                    string.push_str(format!("\n    {:?}: {:?}", message_type, attributes).as_str());
                }
            }
            Ok(string)
        }
        None => Ok(String::from("device server running=None"))
    }
}

// return battery status
async fn battery_status_handler(application_state_db: ApplicationStateDb) -> Result<impl warp::Reply, warp::Rejection> {
    let application_state_mutex = application_state_db.read().await;
    match application_state_mutex.as_ref() {
        Some(application_state) => {
            let mut string = String::new();
            for device in application_state.client.devices() {
                let battery_level = if device.message_attributes().message_allowed(&ButtplugDeviceMessageType::BatteryLevelCmd) {
                    device.battery_level().await.ok()
                } else {
                    None
                };
                string.push_str(format!("{}:{}\n", device.name(), battery_level.unwrap_or(-1.0)).as_str());
            }
            Ok(string)
        }
        None => Ok(String::new())
    }
}

// return device config
async fn device_config_handler(application_state_db: ApplicationStateDb) -> Result<impl warp::Reply, warp::Rejection> {
    let application_state_mutex = application_state_db.read().await;
    match application_state_mutex.as_ref() {
        Some(application_state) => {
            let mut string = String::new();
            for (tag, motor) in application_state.configuration.tags.iter() {
                string.push_str(format!("{};{};{}\n", tag, motor.device_name, motor.feature_type).as_str());
            }
            Ok(string)
        }
        None => Ok(String::new())
    }
}

// haptic websocket handler
async fn haptic_handler(
    websocket: warp::ws::WebSocket,
    application_state_db: ApplicationStateDb,
    watchdog_time: WatchdogTimeoutDb,
) {
    println!("{}: client connected", LOG_PREFIX_HAPTIC_ENDPOINT);
    let (_, mut rx) = websocket.split();
    while let Some(result) = rx.next().await {
        let message = match result {
            Ok(message) => message,
            Err(e) => {
                eprintln!("{}: message read error: {:?}", LOG_PREFIX_HAPTIC_ENDPOINT, e);
                break;
            }
        };
        let message = match message.to_str() {
            Ok(str) => str, // should only succeed for Text() type messages
            Err(_) => {
                if message.is_binary() {
                    eprintln!("{}: received unexpected binary message: {:?}", LOG_PREFIX_HAPTIC_ENDPOINT, message);
                } else if message.is_close() {
                    println!("{}: client closed connection", LOG_PREFIX_HAPTIC_ENDPOINT);
                    return; // stop reading input from the client if they close the connection
                } else if message.is_ping() || message.is_pong() {
                    // do nothing, as there is no need to log ping or pong messages
                } else {
                    /* Text, Binary, Ping, Pong, Close
                     * That should be all the message types, but unfortunately the message type enum
                     * is private so making this check exhaustive is not enforced by the compiler.
                     * In theory the application state should still be fine here, so I don't panic
                     */
                    eprintln!("{}: received unhandled message type: {:?}", LOG_PREFIX_HAPTIC_ENDPOINT, message);
                }

                continue;
            }
        };

        let application_state_mutex = application_state_db.read().await;
        if let Some(application_state) = application_state_mutex.as_ref() {
            let device_map = build_vibration_map(&application_state.configuration, message);

            let mut device_map = match device_map {
                Ok(map) => map,
                Err(e) => {
                    eprintln!("{}: error parsing command: {}", LOG_PREFIX_HAPTIC_ENDPOINT, e);
                    continue;
                }
            };

            for device in application_state.client.devices() {
                if let Some(motor_settings) = device_map.remove(device.name()) {
                    let MotorSettings {
                        scalar_map,
                        rotate_map,
                        linear_map,
                        contraction_hack,
                    } = motor_settings;

                    if !scalar_map.is_empty() {
                        match device.scalar(&ScalarCommand::ScalarMap(scalar_map)).await {
                            Ok(()) => (),
                            Err(e) => eprintln!("{}: error sending command {:?}", LOG_PREFIX_HAPTIC_ENDPOINT, e)
                        }
                    }
                    if !rotate_map.is_empty() {
                        match device.rotate(&RotateCommand::RotateMap(rotate_map)).await {
                            Ok(()) => (),
                            Err(e) => eprintln!("{}: error sending command {:?}", LOG_PREFIX_HAPTIC_ENDPOINT, e)
                        }
                    }
                    if !linear_map.is_empty() {
                        match device.linear(&LinearCommand::LinearMap(linear_map)).await {
                            Ok(()) => (),
                            Err(e) => eprintln!("{}: error sending command {:?}", LOG_PREFIX_HAPTIC_ENDPOINT, e)
                        }
                    }
                    if let Some(air_level) = contraction_hack {
                        todo!()
                        //let command = format!("Air:Level:{};", air_level);
                        //device.raw_write(Endpoint::Tx, command.into(), false).await.expect("unable to contract max");
                    }
                }; // else, ignore this device
            }
            drop(application_state_mutex); // prevent this section from requiring two locks
            watchdog::feed(&watchdog_time).await;
        } // else, no server connected, so send no commands
    }
    println!("{}: client connection lost", LOG_PREFIX_HAPTIC_ENDPOINT);
}

/* convert a command into a tree structure more usable by the Buttplug api
 * The input looks something like this, where 'i' and 'o' are motor tags:
 *
 * "i:0.6;o:0.0"
 *
 * The output looks something like this:
 *
 * Device1:
 *    Motor1Index: Motor1Strength
 *    Motor2Index: Motor2Strength
 * Device2:
 *    Motor1Index: Motor1Strength
 *    Motor2Index: Motor2Strength
 */
fn build_vibration_map(configuration: &ConfigurationV3, command: &str) -> Result<HashMap<String, MotorSettings>, String> {
    let mut devices: HashMap<String, MotorSettings> = HashMap::new();

    for line in command.split_terminator(';') {
        let mut split_line = line.split(':');
        let tag = match split_line.next() {
            Some(tag) => tag,
            None => return Err(format!("could not extract motor tag from {}", line))
        };
        match configuration.motor_from_tag(tag) {
            Some(motor) => {
                match &motor.feature_type {
                    MotorTypeV3::Scalar(actuator_type) => {
                        let intensity = match split_line.next() {
                            Some(tag) => tag,
                            None => return Err(format!("could not extract motor intensity from {}", line))
                        };
                        let intensity = match intensity.parse::<f64>() {
                            Ok(f) => f.filter_nan().clamp(0.0, 1.0),
                            Err(e) => return Err(format!("could not parse motor intensity from {}: {:?}", intensity, e))
                        };

                        devices.entry(motor.device_name.clone())
                            .or_insert_with(MotorSettings::default)
                            .scalar_map
                            .insert(motor.feature_index, (intensity, actuator_type.to_buttplug()));
                    }
                    MotorTypeV3::Linear => {
                        let duration = match split_line.next() {
                            Some(tag) => tag,
                            None => return Err(format!("could not extract motor duration from {}", line))
                        };
                        let duration = match duration.parse::<u32>() {
                            Ok(u) => u,
                            Err(e) => return Err(format!("could not parse motor duration from {}: {:?}", duration, e))
                        };

                        let position = match split_line.next() {
                            Some(tag) => tag,
                            None => return Err(format!("could not extract motor position from {}", line))
                        };
                        let position = match position.parse::<f64>() {
                            Ok(f) => f.filter_nan().clamp(0.0, 1.0),
                            Err(e) => return Err(format!("could not parse motor position from {}: {:?}", position, e))
                        };

                        devices.entry(motor.device_name.clone())
                            .or_insert_with(MotorSettings::default)
                            .linear_map
                            .insert(motor.feature_index, (duration, position));
                    }
                    MotorTypeV3::Rotation => {
                        let speed = match split_line.next() {
                            Some(tag) => tag,
                            None => return Err(format!("could not extract motor speed from {}", line))
                        };
                        let mut speed = match speed.parse::<f64>() {
                            Ok(f) => f.filter_nan().clamp(-1.0, 1.0),
                            Err(e) => return Err(format!("could not parse motor speed from {}: {:?}", speed, e))
                        };

                        let direction = speed >= 0.0;
                        if !direction {
                            speed = -speed;
                        }

                        devices.entry(motor.device_name.clone())
                            .or_insert_with(MotorSettings::default)
                            .rotate_map
                            .insert(motor.feature_index, (speed, direction));
                    }
                }
            }
            None => eprintln!("{}: ignoring unknown motor tag {}", LOG_PREFIX_HAPTIC_ENDPOINT, tag)
        };
    };

    // Ok(&mut devices)
    Ok(devices)
}

trait FloatExtensions {
    fn filter_nan(self) -> Self;
}

impl FloatExtensions for f64 {
    fn filter_nan(self) -> f64 {
        if self.is_nan() {
            0.0
        } else {
            self
        }
    }
}

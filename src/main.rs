// Copyright 2022 runtime-shady-backroom and buttplug-lite contributors.
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

#![windows_subsystem = "windows"]

#[macro_use]
extern crate lazy_static;

use std::{convert, fs};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::time::Duration;

use buttplug::client::{ButtplugClient, ButtplugClientDevice, ButtplugClientEvent, LinearCommand, RotateCommand, ScalarCommand};
use buttplug::core::connector::ButtplugInProcessClientConnectorBuilder;
use buttplug::core::message::{ButtplugDeviceMessageType, ClientGenericDeviceMessageAttributes};
use buttplug::server::ButtplugServerBuilder;
use buttplug::server::device::hardware::communication::btleplug::BtlePlugCommunicationManagerBuilder;
use buttplug::server::device::hardware::communication::lovense_connect_service::LovenseConnectServiceCommunicationManagerBuilder;
use buttplug::server::device::hardware::communication::lovense_dongle::{LovenseHIDDongleCommunicationManagerBuilder, LovenseSerialDongleCommunicationManagerBuilder};
use buttplug::server::device::hardware::communication::serialport::SerialPortCommunicationManagerBuilder;
use chrono::Local;
use clap::Parser;
use directories::ProjectDirs;
use futures::StreamExt;
use semver::Version;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot::Sender;
use tokio::task;
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::util::SubscriberInitExt;
use warp::Filter;

use crate::cli_args::CliArgs;
use crate::configuration_minimal::ConfigurationMinimal;
use crate::configuration_v2::ConfigurationV2;
use crate::configuration_v3::{ActuatorType, ConfigurationV3, MotorConfigurationV3, MotorTypeV3};
use crate::device_status::DeviceStatus;
use crate::gui::subscription::{ApplicationStatusEvent, ApplicationStatusSubscriptionProvider};
use crate::gui::window::TaggedMotor;
use crate::motor_settings::MotorSettings;
use crate::watchdog::WatchdogTimeoutDb;

mod configuration_v2;
mod watchdog;
mod gui;
mod executor;
mod motor_settings;
mod device_status;
mod cli_args;
mod configuration_v3;
mod configuration_minimal;
mod update_checker;


pub const CONFIG_VERSION: i32 = 3;
pub const MAXIMUM_LOG_FILES: usize = 50;

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
static LOG_DIR_NAME: &str = "logs";

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

    let log_filter = if let Some(log_filter_string) = args.log_filter {
        // user is providing a custom filter and not using my verbosity presets at all
        EnvFilter::try_new(log_filter_string).expect("failed to parse user-provided log filter")
    } else if args.verbose == 0 {
        // I get info, everything else gets warn
        EnvFilter::try_new("warn,buttplug_lite=info").unwrap()
    } else if args.verbose == 1 {
        // my debug logging, buttplug's info logging, everything gets warn
        EnvFilter::try_new("warn,buttplug=info,buttplug::server::device::server_device_manager_event_loop=warn,buttplug_derive=info,buttplug_lite=debug").unwrap()
    } else if args.verbose == 2 {
        // my + buttplug's debug logging, everything gets info
        EnvFilter::try_new("info,buttplug=debug,buttplug_derive=debug,buttplug_lite=debug").unwrap()
    } else if args.verbose == 3 {
        // everything gets debug
        EnvFilter::try_new("debug").unwrap()
    } else {
        // dear god everything gets trace
        EnvFilter::try_new("trace").unwrap()
    };

    let _appender_guard = if args.stdout {
        init_console_logging(log_filter);
        None
    } else {
        match create_log_dir_path() {
            Ok(log_dir_path) => {
                let file_appender = tracing_appender::rolling::never(log_dir_path, get_log_file_name());
                let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
                tracing_subscriber::fmt()
                    .with_ansi(false)
                    .with_writer(non_blocking)
                    .with_env_filter(log_filter)
                    .finish()
                    .init();
                Some(guard)
            }
            Err(e) => {
                init_console_logging(log_filter);
                warn!("File-based logging failed. Falling back to stdout: {e}");
                None
            }
        }
    };
    // now we can use tracing to log. Any tracing logs before this point go nowhere.

    info!("initializing {} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    let local_version = Version::parse(env!("CARGO_PKG_VERSION")).expect("Local version didn't follow semver!");
    let update_url = match update_checker::check_for_update().await {
        Ok(response) => {
            info!("Update Url: {:?}", response.html_url);
            info!("Update Version: {:?}", response.tag_name);
            match Version::parse(&response.tag_name) {
                Ok(remote_version) => {
                    match remote_version.cmp(&local_version) {
                        Ordering::Greater => {
                            // we are behind
                            info!("Local version is outdated.");
                            Some(response.html_url)
                        }
                        Ordering::Less => {
                            // we are NEWER than remote
                            info!("Local version is NEWER than remote version!");
                            None
                        }
                        Ordering::Equal => {
                            // we are up to date
                            info!("We are up to date.");
                            None
                        }
                    }
                }
                Err(e) => {
                    info!("Error parsing remote version: {e:?}");
                    None
                }
            }
        }
        Err(e) => {
            info!("Failed to get latest version info: {e:?}");
            None
        }
    };

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
        let mut interval = tokio::time::interval(Duration::from_secs(30));
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
                    info!("shutting down web server")
                });

            let shutdown_message = match server {
                Ok((address, warp_future)) => {
                    info!("starting web server on {address}");

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
                    error!("Failed to start web server: {e:?}");
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
        gui::window::run(application_state_db.clone(), warp_shutdown_initiate_tx, initial_devices, subscription, update_url); // blocking call

        // NOTE: iced hard kills the application when the windows is closed!
        // That means this code is unreachable.
        // As far as I'm aware it is currently impossible to register any sort of shutdown
        // hook/return/signal from iced once you sacrifice your main thread.
        // For now this is fine, as we don't have any super sensitive shutdown code to run,
        // especially with the buttplug server being (apparently?) unstoppable.
    }

    // at this point we begin cleaning up resources for shutdown
    info!("shutting down...");

    // but first, wait for warp to close
    if let Err(e) = warp_shutdown_complete_rx.await {
        info!("error shutting down warp: {e:?}")
    }

    // it's be nice if I could shut down buttplug with `server.shutdown()`, but I'm forced to give server ownership to the connector
    // it'd be nice if I could shut down buttplug with `connector.server_ref().shutdown();`, but I'm forced to give connector ownership to the client
    let mut application_state_mutex = application_state_db.write().await;
    if let Some(application_state) = application_state_mutex.deref_mut() {
        if let Err(e) = application_state.client.disconnect().await {
            warn!("Unable to disconnect internal client from internal server: {e}");
        }
    }
}

fn init_console_logging(log_filter: EnvFilter) {
    tracing_subscriber::fmt()
        .with_env_filter(log_filter)
        .finish()
        .init()
}

fn get_config_dir() -> PathBuf {
    ProjectDirs::from("io.github", "runtime-shady-backroom", env!("CARGO_PKG_NAME"))
        .expect("unable to locate configuration directory")
        .config_dir()
        .into()
}

fn create_config_file_path() -> PathBuf {
    let config_dir_path: PathBuf = get_config_dir();
    fs::create_dir_all(config_dir_path.as_path()).expect("failed to create configuration directory");
    config_dir_path.join(CONFIG_FILE_NAME)
}

fn get_log_file_name() -> String {
    Local::now().format("%Y-%m-%d_%H-%M-%S.log").to_string()
}

fn get_log_dir() -> PathBuf {
    ProjectDirs::from("io.github", "runtime-shady-backroom", env!("CARGO_PKG_NAME"))
        .expect("unable to locate configuration directory")
        .data_dir()
        .join(LOG_DIR_NAME)
}

fn create_log_dir_path() -> std::io::Result<PathBuf> {
    let log_dir_path: PathBuf = get_log_dir();
    fs::create_dir_all(log_dir_path.as_path())?;
    clean_up_old_logs(log_dir_path.as_path())?;

    // new log file
    Ok(log_dir_path)
}

fn clean_up_old_logs(path: &Path) -> std::io::Result<()> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().map(|ext| ext == "log").unwrap_or(false) {
            paths.push(path);
        }
    }
    paths.sort_unstable();
    if let Some(logs_to_delete) = paths.len().checked_sub(MAXIMUM_LOG_FILES) {
        for path in paths.into_iter().take(logs_to_delete) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn get_backup_config_file_path(version: i32) -> PathBuf {
    get_config_dir().join(format!("backup_config_v{version}.toml"))
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

    // the following things can be stolen from the server and may be useful for duplicate device detection
    //let device_manager = server.device_manager();
    //let event_stream = server.event_stream();

    let connector = ButtplugInProcessClientConnectorBuilder::default()
        .server(server)
        .finish();

    match buttplug_client.connect(connector).await {
        Ok(()) => {
            info!("{LOG_PREFIX_BUTTPLUG_SERVER}: Device server started!");
            let mut event_stream = buttplug_client.event_stream();
            match buttplug_client.start_scanning().await {
                Ok(()) => info!("{LOG_PREFIX_BUTTPLUG_SERVER}: starting device scan"),
                Err(e) => warn!("{LOG_PREFIX_BUTTPLUG_SERVER}: scan failure: {e:?}")
            };

            // reuse old config, or load from disk if this is the initial connection
            let previous_state = application_state_mutex.deref_mut().take();
            let configuration = match previous_state {
                Some(ApplicationState { configuration, client: _ }) => configuration,
                None => {
                    info!("{}: Attempting to load config from {:?}", LOG_PREFIX_BUTTPLUG_SERVER, *CONFIG_DIR_FILE_PATH);
                    let loaded_configuration: Result<ConfigurationMinimal, String> = fs::read_to_string(CONFIG_DIR_FILE_PATH.as_path())
                        .map_err(|e| format!("{e:?}"))
                        .and_then(|string| toml::from_str(&string).map_err(|e| format!("{e:?}")));
                    let configuration: ConfigurationV3 = match loaded_configuration {
                        Ok(configuration) => {
                            let loaded_configuration: Result<ConfigurationV3, String> = if configuration.version < 3 {
                                fs::copy(CONFIG_DIR_FILE_PATH.as_path(), get_backup_config_file_path(configuration.version)).expect("failed to back up config");
                                info!("converting v{} config to v{}", configuration.version, CONFIG_VERSION);
                                fs::read_to_string(CONFIG_DIR_FILE_PATH.as_path())
                                    .map_err(|e| format!("{e:?}"))
                                    .and_then(|string| toml::from_str::<ConfigurationV2>(&string).map_err(|e| format!("{e:?}")))
                                    .map(|config| config.into())
                            } else {
                                fs::read_to_string(CONFIG_DIR_FILE_PATH.as_path())
                                    .map_err(|e| format!("{e:?}"))
                                    .and_then(|string| toml::from_str::<ConfigurationV3>(&string).map_err(|e| format!("{e:?}")))
                            };

                            match loaded_configuration {
                                Ok(configuration) => configuration,
                                Err(e) => {
                                    // attempt to backup old config file when read fails
                                    fs::copy(CONFIG_DIR_FILE_PATH.as_path(), get_backup_config_file_path(configuration.version)).expect("failed to back up config");
                                    warn!("falling back to default config due to error: {e}");
                                    ConfigurationV3::default()
                                }
                            }
                        }
                        Err(e) => {
                            warn!("falling back to default config due to error: {e}");
                            ConfigurationV3::default()
                        }
                    };
                    info!("{LOG_PREFIX_BUTTPLUG_SERVER}: Loaded configuration v{} from disk", configuration.version);

                    if configuration.is_outdated() {
                        let new_configuration = configuration.new_with_current_version();
                        match save_configuration(&new_configuration).await {
                            Ok(_) => {
                                info!("{LOG_PREFIX_BUTTPLUG_SERVER}: Migrated configuration to new directory");
                                new_configuration
                            }
                            Err(e) => {
                                warn!("{LOG_PREFIX_BUTTPLUG_SERVER}: Error migrating configuration to new directory: {e}");
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
                            info!("{LOG_PREFIX_BUTTPLUG_SERVER}: device connected: {} #{}", dev.name(), dev.index());
                            application_status_event_sender.send(ApplicationStatusEvent::DeviceAdded).expect("failed to send device added event");
                        }
                        ButtplugClientEvent::DeviceRemoved(dev) => {
                            info!("{LOG_PREFIX_BUTTPLUG_SERVER}: device disconnected: {} #{}", dev.name(), dev.index());
                            application_status_event_sender.send(ApplicationStatusEvent::DeviceRemoved).expect("failed to send device removed event");
                        }
                        ButtplugClientEvent::PingTimeout => info!("{LOG_PREFIX_BUTTPLUG_SERVER}: ping timeout"),
                        ButtplugClientEvent::Error(e) => info!("{LOG_PREFIX_BUTTPLUG_SERVER}: server error: {e:?}"),
                        ButtplugClientEvent::ScanningFinished => info!("{LOG_PREFIX_BUTTPLUG_SERVER}: device scan finished"),
                        ButtplugClientEvent::ServerConnect => info!("{LOG_PREFIX_BUTTPLUG_SERVER}: server connected"),
                        ButtplugClientEvent::ServerDisconnect => {
                            info!("{LOG_PREFIX_BUTTPLUG_SERVER}: server disconnected");
                            let mut application_state_mutex = application_state_db.write().await;
                            *application_state_mutex = None; // not strictly required but will give more sane error messages
                            break;
                        }
                    },
                    None => warn!("{LOG_PREFIX_BUTTPLUG_SERVER}: error reading haptic event")
                };
            }
        }
        Err(e) => warn!("{LOG_PREFIX_BUTTPLUG_SERVER}: failed to connect to server. Will retry shortly... ({e:?})") // will try to reconnect later, may not need to log this error
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
                    .map_err(|e| format!("{e:?}"))?;
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
        fs::write(CONFIG_DIR_FILE_PATH.as_path(), serialized_config).map_err(|e| format!("{e:?}"))
    }).await
        .map_err(|e| format!("{e:?}"))
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

#[inline(always)]
fn name_from_device(device: &ButtplugClientDevice) -> String {
    device.name().clone()
    // once we want to handle duplicate devices:
    //format!("{}#{}", device.name(), device.index())
}

fn motor_configuration_from_devices(devices: Vec<Arc<ButtplugClientDevice>>) -> Vec<MotorConfigurationV3> {
    let mut motor_configuration_count: usize = 0;
    for device in devices.iter() {
        motor_configuration_count += device.message_attributes().scalar_cmd().as_ref().map_or(0, |v| v.len());
        motor_configuration_count += device.message_attributes().rotate_cmd().as_ref().map_or(0, |v| v.len());
        motor_configuration_count += device.message_attributes().linear_cmd().as_ref().map_or(0, |v| v.len());
    }

    let mut motor_configurations: Vec<MotorConfigurationV3> = Vec::with_capacity(motor_configuration_count);

    let empty_vec = Vec::new();

    for device in devices.into_iter() {
        let scalar_cmds: &Vec<ClientGenericDeviceMessageAttributes> = device.message_attributes().scalar_cmd().as_ref().unwrap_or(&empty_vec);
        for index in 0..scalar_cmds.len() {
            let message_attributes: &ClientGenericDeviceMessageAttributes = scalar_cmds.get(index).expect("I didn't know a vec could change mid-iteration");
            let actuator_type: ActuatorType = message_attributes.actuator_type().into();
            let motor_config = MotorConfigurationV3 {
                device_name: name_from_device(&device),
                feature_type: MotorTypeV3::Scalar { actuator_type },
                feature_index: index as u32,
            };
            motor_configurations.push(motor_config);
        }

        let rotate_cmds: &Vec<ClientGenericDeviceMessageAttributes> = device.message_attributes().rotate_cmd().as_ref().unwrap_or(&empty_vec);
        for index in 0..rotate_cmds.len() {
            let motor_config = MotorConfigurationV3 {
                device_name: name_from_device(&device),
                feature_type: MotorTypeV3::Rotation,
                feature_index: index as u32,
            };
            motor_configurations.push(motor_config);
        }

        let linear_cmds: &Vec<ClientGenericDeviceMessageAttributes> = device.message_attributes().linear_cmd().as_ref().unwrap_or(&empty_vec);
        for index in 0..linear_cmds.len() {
            let motor_config = MotorConfigurationV3 {
                device_name: name_from_device(&device),
                feature_type: MotorTypeV3::Linear,
                feature_index: index as u32,
            };
            motor_configurations.push(motor_config);
        }
    }

    motor_configurations
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

    let motors = motor_configuration_from_devices(devices);

    DeviceList {
        motors,
        devices: device_statuses,
    }
}

// return a device status summary
async fn haptic_status_handler(application_state_db: ApplicationStateDb) -> Result<impl warp::Reply, warp::Rejection> {
    let application_state_mutex = application_state_db.read().await;
    match application_state_mutex.as_ref() {
        Some(application_state) => {
            let connected = application_state.client.connected();
            let mut string = format!("device server running={connected}");
            for device in application_state.client.devices() {
                string.push_str(format!("\n  {}", device.name()).as_str());
                if let Some(display_name) = device.display_name() {
                    string.push_str(format!(" [{display_name}]").as_str());
                }

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
                    string.push_str(format!("\n    {message_type:?}: {attributes:?}").as_str());
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
    info!("{LOG_PREFIX_HAPTIC_ENDPOINT}: client connected");
    let (_, mut rx) = websocket.split();
    while let Some(result) = rx.next().await {
        let message = match result {
            Ok(message) => message,
            Err(e) => {
                warn!("{LOG_PREFIX_HAPTIC_ENDPOINT}: message read error: {e:?}");
                break;
            }
        };
        let message = match message.to_str() {
            Ok(str) => str, // should only succeed for Text() type messages
            Err(_) => {
                if message.is_binary() {
                    warn!("{LOG_PREFIX_HAPTIC_ENDPOINT}: received unexpected binary message: {message:?}");
                } else if message.is_close() {
                    info!("{LOG_PREFIX_HAPTIC_ENDPOINT}: client closed connection");
                    return; // stop reading input from the client if they close the connection
                } else if message.is_ping() || message.is_pong() {
                    // do nothing, as there is no need to log ping or pong messages
                } else {
                    /* Text, Binary, Ping, Pong, Close
                     * That should be all the message types, but unfortunately the message type enum
                     * is private so making this check exhaustive is not enforced by the compiler.
                     * In theory the application state should still be fine here, so I don't panic
                     */
                    warn!("{LOG_PREFIX_HAPTIC_ENDPOINT}: received unhandled message type: {message:?}");
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
                    debug!("{LOG_PREFIX_HAPTIC_ENDPOINT}: error parsing command: {e}");
                    continue;
                }
            };

            for device in application_state.client.devices() {
                if let Some(motor_settings) = device_map.remove(device.name()) {
                    let MotorSettings {
                        scalar_map,
                        rotate_map,
                        linear_map,
                    } = motor_settings;

                    if !scalar_map.is_empty() {
                        match device.scalar(&ScalarCommand::ScalarMap(scalar_map)).await {
                            Ok(()) => (),
                            Err(e) => warn!("{LOG_PREFIX_HAPTIC_ENDPOINT}: error sending command {e:?}",)
                        }
                    }
                    if !rotate_map.is_empty() {
                        match device.rotate(&RotateCommand::RotateMap(rotate_map)).await {
                            Ok(()) => (),
                            Err(e) => warn!("{LOG_PREFIX_HAPTIC_ENDPOINT}: error sending command {e:?}")
                        }
                    }
                    if !linear_map.is_empty() {
                        match device.linear(&LinearCommand::LinearMap(linear_map)).await {
                            Ok(()) => (),
                            Err(e) => warn!("{LOG_PREFIX_HAPTIC_ENDPOINT}: error sending command {e:?}")
                        }
                    }
                }; // else, ignore this device
            }
            drop(application_state_mutex); // prevent this section from requiring two locks
            watchdog::feed(&watchdog_time).await;
        } // else, no server connected, so send no commands
    }
    info!("{LOG_PREFIX_HAPTIC_ENDPOINT}: client connection lost");
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
            None => return Err(format!("could not extract motor tag from {line}"))
        };
        match configuration.motor_from_tag(tag) {
            Some(motor) => {
                match &motor.feature_type {
                    MotorTypeV3::Scalar { actuator_type } => {
                        let intensity = match split_line.next() {
                            Some(tag) => tag,
                            None => return Err(format!("could not extract motor intensity from {line}"))
                        };
                        let intensity = match intensity.parse::<f64>() {
                            Ok(f) => f.filter_nan().clamp(0.0, 1.0),
                            Err(e) => return Err(format!("could not parse motor intensity from {intensity}: {e:?}"))
                        };

                        devices.entry(motor.device_name.clone())
                            .or_insert_with(MotorSettings::default)
                            .scalar_map
                            .insert(motor.feature_index, (intensity, actuator_type.to_buttplug()));
                    }
                    MotorTypeV3::Linear => {
                        let duration = match split_line.next() {
                            Some(tag) => tag,
                            None => return Err(format!("could not extract motor duration from {line}"))
                        };
                        let duration = match duration.parse::<u32>() {
                            Ok(u) => u,
                            Err(e) => return Err(format!("could not parse motor duration from {duration}: {e:?}"))
                        };

                        let position = match split_line.next() {
                            Some(tag) => tag,
                            None => return Err(format!("could not extract motor position from {line}"))
                        };
                        let position = match position.parse::<f64>() {
                            Ok(f) => f.filter_nan().clamp(0.0, 1.0),
                            Err(e) => return Err(format!("could not parse motor position from {position}: {e:?}"))
                        };

                        devices.entry(motor.device_name.clone())
                            .or_insert_with(MotorSettings::default)
                            .linear_map
                            .insert(motor.feature_index, (duration, position));
                    }
                    MotorTypeV3::Rotation => {
                        let speed = match split_line.next() {
                            Some(tag) => tag,
                            None => return Err(format!("could not extract motor speed from {line}"))
                        };
                        let mut speed = match speed.parse::<f64>() {
                            Ok(f) => f.filter_nan().clamp(-1.0, 1.0),
                            Err(e) => return Err(format!("could not parse motor speed from {speed}: {e:?}"))
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
            None => debug!("{LOG_PREFIX_HAPTIC_ENDPOINT}: ignoring unknown motor tag {tag}")
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

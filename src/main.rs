use std::borrow::Borrow;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use buttplug::client::{ButtplugClient, ButtplugClientDevice, ButtplugClientEvent, device::VibrateCommand};
use buttplug::connector::ButtplugInProcessClientConnector;
use buttplug::server::comm_managers::{
    btleplug::BtlePlugCommunicationManager,
    lovense_dongle::{LovenseHIDDongleCommunicationManager, LovenseSerialDongleCommunicationManager},
    xinput::XInputDeviceCommunicationManager,
};
use futures::StreamExt;
use tokio::sync::{mpsc, Mutex, oneshot, RwLock};
use tokio::task;
use warp::Filter;

use configuration::Configuration;

use crate::configuration::{ConfigurationDb, Motor, MotorType};
use crate::watchdog::WatchdogTimeoutDb;
use app_dirs::{AppInfo, AppDataType};
use std::fs;

mod configuration;
mod watchdog;
mod gui;
mod util;

// global state types
pub type HapticConnectorDb = Arc<RwLock<Option<HapticConnector>>>;

// how long to wait before attempting a reconnect to the server
const HAPTIC_SERVER_RECONNECT_DELAY_MILLIS: u64 = 5000;

// name of this client from the buttplug.io server's perspective
const HAPTIC_SERVER_CLIENT_NAME: &str = "in-process-client";

// log prefixes:
const LOG_PREFIX_HAPTIC_ENDPOINT: &str = "/haptic";
const LOG_PREFIX_HAPTIC_SERVER: &str = "haptic_server";

const APP_INFO: AppInfo = AppInfo {
    name: env!("CARGO_PKG_NAME"),
    author: "runtime"
};

// eventually I'd like some way to get a ref to the server in here
pub struct HapticConnector {
    pub client: ButtplugClient,
}

#[derive(Debug)]
pub enum ShutdownMessage {
    Restart,
    Shutdown,
}

#[tokio::main]
async fn main() {
    println!("initializing {} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    let haptic_watchdog_db: WatchdogTimeoutDb = Arc::new(Mutex::new(None));
    let haptic_connector_db: HapticConnectorDb = Arc::new(RwLock::new(None));

    let config_dir_path = app_dirs::get_app_root(AppDataType::UserConfig, &APP_INFO).expect("unable to locate configuration directory");
    fs::create_dir_all(config_dir_path.as_path()).expect("failed to create configuration directory");
    let config_file_path = config_dir_path.join("config.toml");

    let loaded_configuration = fs::read_to_string(config_file_path.as_path())
        .map_err(|e| format!("{:?}", e))
        .and_then(|string| toml::from_str(string.borrow()).map_err(|e| format!("{:?}", e)));
    let configuration = match loaded_configuration {
        Ok(configuration) => configuration,
        Err(e) => {
            //TODO: do not clobber old config when read fails
            eprintln!("falling back to default config due to error: {}", e);
            Configuration::default()
        }
    };

    //TODO: move config saving elsewhere
    let serialized_config = toml::to_string(&configuration).expect("failed to serialize configuration");
    fs::write(config_file_path, serialized_config).expect("failed to save configuration");

    println!("{:?}", configuration);

    let proxy_server_address: SocketAddr = ([127, 0, 0, 1], configuration.port).into();
    let configuration_db: ConfigurationDb = Arc::new(RwLock::new(configuration));

    // GET /hapticstatus => 200 OK with body containing haptic status
    let hapticstatus = warp::path("hapticstatus")
        .and(warp::get())
        .and(with_db(haptic_connector_db.clone()))
        .and_then(haptic_status_handler);

    // WEBSOCKET /haptic
    let haptic = warp::path("haptic")
        .and(warp::ws())
        .and(with_db(haptic_connector_db.clone()))
        .and(with_db(haptic_watchdog_db.clone()))
        .and(with_db(configuration_db))
        .map(|ws: warp::ws::Ws, haptic_connector_db: HapticConnectorDb, haptic_watchdog_db: WatchdogTimeoutDb, configuration_db: ConfigurationDb| {
            ws.on_upgrade(|ws| haptic_handler(ws, haptic_connector_db, haptic_watchdog_db, configuration_db))
        });

    let routes = hapticstatus
        .or(haptic);

    watchdog::start(haptic_watchdog_db, haptic_connector_db.clone());

    // connector clone moved into reconnect task
    let reconnector_haptic_connector_clone = haptic_connector_db.clone();

    // spawn the server reconnect task
    // when the server is connected this functions as the event reader
    // when the server is disconnected it attempts to reconnect after a delay
    task::spawn(async move {
        loop {
            // possible states of server (haptic_client_db.lock().await):
            // None // need to perform initial connection
            // Some(haptic_client).connected() == true // bad state, reconnect anyway
            // Some(haptic_client).connected() == false // reconnect needed
            // in summary, we don't care what the state is and we (re)connect regardless
            start_haptic_server(reconnector_haptic_connector_clone.clone()).await; // will "block" until disconnect
            tokio::time::sleep(Duration::from_millis(HAPTIC_SERVER_RECONNECT_DELAY_MILLIS)).await; // reconnect delay
        }
    });


    let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel::<ShutdownMessage>();

    task::spawn_blocking(move || {
        gui::run(shutdown_tx); // blocking call
    });

    // loop handles restarting the warp server if needed
    loop {
        // used to proxy the signal from the mpsc into the graceful_shutdown closure later
        // this is needed because we cannot move the mpsc consumer
        let (oneshot_tx, oneshot_rx) = oneshot::channel::<()>();

        let server = warp::serve(routes.clone())
            .try_bind_with_graceful_shutdown(proxy_server_address, async move {
                oneshot_rx.await.expect("error receiving shutdown signal");
            });

        let shutdown_message = match server {
            Ok((address, warp_future)) => {
                println!("starting web server on {}", address);

                // run warp in the background
                task::spawn(async move {
                    warp_future.await;
                });

                // sacrifice main thread to shutdown trigger bullshit
                let signal = shutdown_rx.recv().await.unwrap_or(ShutdownMessage::Shutdown);
                oneshot_tx.send(()).expect("error transmitting shutdown signal");
                signal
            }
            Err(e) => {
                eprintln!("Failed to start web server: {:?}", e);
                ShutdownMessage::Shutdown
            }
        };

        if let ShutdownMessage::Shutdown = shutdown_message {
            break;
        }
        // otherwise we go again
    }

    // at this point we begin cleaning up resources for shutdown
    println!("shutting down...");

    let haptic_connector_mutex = haptic_connector_db.read().await;
    if let Some(connector) = haptic_connector_mutex.as_ref() {
        connector.client.stop_all_devices().await.expect("failed to halt devices before exit");
        connector.client.stop_scanning().await.expect("failed to stop scanning before exit");
        connector.client.disconnect().await.expect("failed to disconnect from internal haptic server");
    }
}

// start server, then while running process events
// returns only when we disconnect from the server
async fn start_haptic_server(haptic_connector_db: HapticConnectorDb) {
    let mut haptic_connector_mutex = haptic_connector_db.write().await;
    let haptic_client = ButtplugClient::new(HAPTIC_SERVER_CLIENT_NAME);

    let connector = ButtplugInProcessClientConnector::default();

    let server = connector.server_ref();
    server.add_comm_manager::<BtlePlugCommunicationManager>().unwrap();
    server.add_comm_manager::<LovenseHIDDongleCommunicationManager>().unwrap();
    server.add_comm_manager::<LovenseSerialDongleCommunicationManager>().unwrap();

    #[cfg(target_os = "windows")] {
        server.add_comm_manager::<XInputDeviceCommunicationManager>().unwrap();
    }

    match haptic_client.connect(connector).await {
        Ok(()) => {
            println!("{}: Device server started!", LOG_PREFIX_HAPTIC_SERVER);
            let mut event_stream = haptic_client.event_stream();
            match haptic_client.start_scanning().await {
                Ok(()) => println!("{}: starting device scan", LOG_PREFIX_HAPTIC_SERVER),
                Err(e) => eprintln!("{}: scan failure: {:?}", LOG_PREFIX_HAPTIC_SERVER, e)
            };
            *haptic_connector_mutex = Some(HapticConnector { client: haptic_client });
            drop(haptic_connector_mutex); // prevent this section from requiring two locks
            loop {
                match event_stream.next().await {
                    Some(event) => match event {
                        ButtplugClientEvent::DeviceAdded(dev) => println!("{}: device connected: {}", LOG_PREFIX_HAPTIC_SERVER, dev.name),
                        ButtplugClientEvent::DeviceRemoved(dev) => println!("{}: device disconnected: {}", LOG_PREFIX_HAPTIC_SERVER, dev.name),
                        ButtplugClientEvent::PingTimeout => println!("{}: ping timeout", LOG_PREFIX_HAPTIC_SERVER),
                        ButtplugClientEvent::Error(e) => println!("{}: server error: {:?}", LOG_PREFIX_HAPTIC_SERVER, e),
                        ButtplugClientEvent::ScanningFinished => println!("{}: device scan finished", LOG_PREFIX_HAPTIC_SERVER),
                        ButtplugClientEvent::ServerConnect => println!("{}: server connected", LOG_PREFIX_HAPTIC_SERVER),
                        ButtplugClientEvent::ServerDisconnect => {
                            println!("{}: server disconnected", LOG_PREFIX_HAPTIC_SERVER);
                            let mut haptic_connector_mutex = haptic_connector_db.write().await;
                            *haptic_connector_mutex = None; // not strictly required but will give more sane error messages
                            break;
                        }
                    },
                    None => eprintln!("{}: error reading haptic event", LOG_PREFIX_HAPTIC_SERVER)
                };
            }
        }
        Err(_) => () // will try to reconnect later, no need to log this error
    }
}

fn with_db<T: Clone + Send>(db: T) -> impl Filter<Extract=(T, ), Error=std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

pub async fn get_devices(haptic_connector_db: &HapticConnectorDb) -> Vec<Motor> {
    let haptic_connector_mutex = haptic_connector_db.read().await;
    match haptic_connector_mutex.as_ref() {
        Some(haptic_connector) => {
            haptic_connector.client.devices().into_iter()
                .flat_map(|device| {
                    MotorType::iter()
                        .flat_map(move |feature_type| {
                            let device_name = device.name.clone();
                            let feature_count = device_feature_count_by_type(feature_type, device.borrow());
                            let feature_range = 0..feature_count;
                            feature_range.into_iter()
                                .map(move |feature_index| {
                                    Motor {
                                        device_name: device_name.clone(),
                                        feature_index,
                                        feature_type: feature_type.clone(),
                                    }
                                })
                        })
                })
                .collect()
        }
        None => Default::default()
    }
}

fn device_feature_count_by_type(device_type: &MotorType, device: &ButtplugClientDevice) -> u32 {
    device.allowed_messages.get(device_type.get_type().borrow())
        .map(|attributes| attributes.feature_count)
        .flatten()
        .unwrap_or(0)
}

// return a device status summary
async fn haptic_status_handler(haptic_connector_db: HapticConnectorDb) -> Result<impl warp::Reply, warp::Rejection> {
    let haptic_connector_mutex = haptic_connector_db.read().await;
    match haptic_connector_mutex.as_ref() {
        Some(haptic_connector) => {
            let connected = haptic_connector.client.connected();
            let mut string = String::from(format!("device server running={}", connected));
            for device in haptic_connector.client.devices() {
                string.push_str(format!("\n  {}", device.name).as_str());
                for (message_type, attributes) in device.allowed_messages.iter() {
                    string.push_str(format!("\n    {:?}: {:?}", message_type, attributes).as_str());
                }
            }
            Ok(string)
        }
        None => Ok(String::from("device server running=None"))
    }
}

// haptic websocket handler
async fn haptic_handler(
    websocket: warp::ws::WebSocket,
    haptic_connector_db: HapticConnectorDb,
    watchdog_time: WatchdogTimeoutDb,
    configuration_db: ConfigurationDb,
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
        let configuration_mutex = configuration_db.read().await;
        let haptic_map = build_haptic_map(configuration_mutex.deref(), message);
        drop(configuration_mutex);

        let mut map = match haptic_map {
            Ok(map) => map,
            Err(e) => {
                eprintln!("{}: error parsing command: {}", LOG_PREFIX_HAPTIC_ENDPOINT, e);
                continue;
            }
        };

        let haptic_connector_mutex = haptic_connector_db.read().await;
        match haptic_connector_mutex.as_ref() {
            Some(haptic_connector) => {
                for device in haptic_connector.client.devices() {
                    match map.remove(device.name.as_str()) {
                        Some(speed_map) => {
                            match device.vibrate(VibrateCommand::SpeedMap(speed_map)).await {
                                Ok(()) => (),
                                Err(e) => eprintln!("{}: error sending command {:?}", LOG_PREFIX_HAPTIC_ENDPOINT, e)
                            }
                        }
                        None => () // ignore this device
                    };
                }
                drop(haptic_connector_mutex); // prevent this section from requiring two locks
                watchdog::feed(&watchdog_time).await;
            }
            None => () // no server connected, so send no commands
        }
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
fn build_haptic_map(configuration: &Configuration, command: &str) -> Result<HashMap<String, HashMap<u32, f64>>, String> {
    let mut devices: HashMap<String, HashMap<u32, f64>> = HashMap::new();

    for line in command.split(';') {
        let mut split_line = line.split(':');
        let tag = match split_line.next() {
            Some(tag) => tag,
            None => return Err(format!("could not extract motor tag from {}", line))
        };
        let intensity = match split_line.next() {
            Some(tag) => tag,
            None => return Err(format!("could not extract motor intensity from {}", line))
        };
        let intensity = match intensity.parse::<f64>() {
            Ok(f) => util::clamp(f),
            Err(e) => return Err(format!("could not parse motor intensity from {}: {:?}", intensity, e))
        };
        match configuration.motor_from_tag(tag.to_owned().borrow()) {
            Some(motor) => {
                if let MotorType::Vibration = motor.feature_type {
                    // make a new submap if needed
                    devices.entry(motor.device_name.clone())
                        .or_insert(HashMap::new())
                        .insert(motor.feature_index, intensity);
                } else {
                    eprintln!("{}: ignoring tag {} because only vibration is supported presently", LOG_PREFIX_HAPTIC_ENDPOINT, tag)
                }
            }
            None => eprintln!("{}: ignoring unknown motor tag {}", LOG_PREFIX_HAPTIC_ENDPOINT, tag)
        };
    };

    // Ok(&mut devices)
    Ok(devices)
}

use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use buttplug::client::{ButtplugClient, ButtplugClientEvent, device::VibrateCommand};
use buttplug::connector::ButtplugInProcessClientConnector;
use buttplug::server::comm_managers::{
    btleplug::BtlePlugCommunicationManager,
    lovense_dongle::{LovenseHIDDongleCommunicationManager, LovenseSerialDongleCommunicationManager},
    xinput::XInputDeviceCommunicationManager,
};
use druid::{AppLauncher, Widget, WindowDesc};
use druid::widget::Label;
use futures::StreamExt;
use tokio::sync::{Mutex, oneshot, RwLock};
use warp::Filter;

// global state types
type WatchdogTimeoutDb = Arc<Mutex<Option<i64>>>;
type HapticConnectorDb = Arc<RwLock<Option<HapticConnector>>>;

// how often the watchdog runs its check
const WATCHDOG_POLL_INTERVAL_MILLIS: u64 = 1000;

// halt devices after this much time with no command received
const WATCHDOG_TIMEOUT_MILLIS: i64 = 10000;

// how long to wait before attempting a reconnect to the server
const HAPTIC_SERVER_RECONNECT_DELAY_MILLIS: u64 = 5000;

// name of this client from the buttplug.io server's perspective
const HAPTIC_SERVER_CLIENT_NAME: &str = "in-process-client";

// log prefixes:
const LOG_PREFIX_HAPTIC_ENDPOINT: &str = "/haptic";
const LOG_PREFIX_HAPTIC_SERVER: &str = "haptic_server";

// encodes the "address" of a specific motor
struct MotorId<'a> {
    device_name: &'a str,
    feature_index: u32,
}

// eventually I'd like some way to get a ref to the server in here
struct HapticConnector {
    client: ButtplugClient,
}

fn build_ui() -> impl Widget<()> {
    Label::new("Hello world")
}

#[tokio::main]
async fn main() {
    println!("initializing {} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    let proxy_server_address: SocketAddr = ([127, 0, 0, 1], 3031).into();

    let haptic_watchdog_db: WatchdogTimeoutDb = Arc::new(Mutex::new(None));
    let haptic_connector_db: HapticConnectorDb = Arc::new(RwLock::new(None));

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
        .map(|ws: warp::ws::Ws, haptic_connector_db: HapticConnectorDb, watchdog_timestamp: WatchdogTimeoutDb| {
            ws.on_upgrade(|ws| haptic_handler(ws, haptic_connector_db, watchdog_timestamp))
        });

    let routes = hapticstatus
        .or(haptic);

    // connector clone moved into watchdog task
    let watchdog_haptic_connector_clone = haptic_connector_db.clone();

    // spawn the watchdog task
    // if too much time passes with no input from the client, this halts all haptic devices
    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(WATCHDOG_POLL_INTERVAL_MILLIS));
        loop {
            interval.tick().await;
            let unix_time = SystemTime::now().duration_since(UNIX_EPOCH)
                .expect("Your system clock is wrong")
                .as_millis();
            let unix_time = i64::try_from(unix_time).expect("System time out of range");
            // probably fine to panic if your system clock is before the unix epoch...

            let mut watchdog_time_mutex = haptic_watchdog_db.lock().await;
            let watchdog_violation = match *watchdog_time_mutex {
                Some(watchdog_time) => unix_time - watchdog_time > WATCHDOG_TIMEOUT_MILLIS,
                None => false
            };
            if watchdog_violation {
                println!("Watchdog violation! Halting all devices. To avoid this send an update at least every {}ms.", WATCHDOG_TIMEOUT_MILLIS);
                *watchdog_time_mutex = None; // this prevents the message from spamming
                drop(watchdog_time_mutex); // prevent this section from requiring two locks
                let haptic_connector_mutex = watchdog_haptic_connector_clone.read().await;
                match haptic_connector_mutex.as_ref() {
                    Some(haptic_connector) => {
                        match haptic_connector.client.stop_all_devices().await {
                            Ok(()) => (),
                            Err(e) => eprintln!("watchdog: error halting devices: {:?}", e)
                        }
                    }
                    None => () // do nothing because there is no server connected
                }
            }
        }
    });

    // connector clone moved into reconnect task
    let reconnector_haptic_connector_clone = haptic_connector_db.clone();

    // spawn the server reconnect task
    // when the server is connected this functions as the event reader
    // when the server is disconnected it attempts to reconnect after a delay
    tokio::task::spawn(async move {
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

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    tokio::task::spawn_blocking(|| {
        AppLauncher::with_window(WindowDesc::new(build_ui)).launch(()).unwrap();

        match shutdown_tx.send(()) {
            Ok(()) => println!("shutdown triggered"),
            Err(()) => panic!("Error triggering shutdown")
        };
    });

    let server = warp::serve(routes)
        .try_bind_with_graceful_shutdown(proxy_server_address, async {
            if let Err(e) = shutdown_rx.await {
                eprintln!("Error waiting for shutdown trigger: {}", e);
            }
        });

    match server {
        Ok((addr, future)) => {
            println!("starting web server on {}", addr);
            future.await; // sacrifice the main thread to warp
        }
        Err(e) => eprintln!("Failed to start web server: {:?}", e)
    }

    // at this point we being cleaning up resources for shutdown
    println!("shutting down...");

    let haptic_connector_mutex = haptic_connector_db.read().await;
    if let Some(connector) = haptic_connector_mutex.as_ref() {
        connector.client.stop_scanning().await.expect("failed to stop scanning before exit");
        connector.client.stop_all_devices().await.expect("failed to halt devices before exit");
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
                println!("{}: waiting for event...", LOG_PREFIX_HAPTIC_SERVER); //TODO: remove when done debugging deadlock
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
                            println!("{}: didn't deadlock!", LOG_PREFIX_HAPTIC_SERVER); //TODO: remove when done debugging deadlock
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
async fn haptic_handler(websocket: warp::ws::WebSocket, haptic_connector_db: HapticConnectorDb, watchdog_time: WatchdogTimeoutDb) {
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

        let mut map = match build_haptic_map(message) {
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

                // feed the watchdog
                let mut watchdog_time_mutex = watchdog_time.lock().await;
                let unix_time = SystemTime::now().duration_since(UNIX_EPOCH)
                    .expect("Your system clock is wrong")
                    .as_millis();
                *watchdog_time_mutex = Some(i64::try_from(unix_time).expect("System time out of range"));
                // probably fine to panic if your system clock is before the unix epoch...
            }
            None => () // no server connected, so send no commands
        }
    }
    println!("{}: client connection lost", LOG_PREFIX_HAPTIC_ENDPOINT);
}

// convert a tag into a full motor id
fn motor_from_tag<'a>(tag: &str) -> Option<MotorId<'a>> {
    match tag {
        "o" => Some(MotorId { device_name: "Lovense Edge", feature_index: 0 }), // edge outer (verified)
        "i" => Some(MotorId { device_name: "Lovense Edge", feature_index: 1 }), // edge inner (verified)
        "h" => Some(MotorId { device_name: "Lovense Hush", feature_index: 0 }), // hush
        "l" => Some(MotorId { device_name: "Lovense Lush", feature_index: 0 }), // lush 2
        "m" => Some(MotorId { device_name: "Lovense Max", feature_index: 0 }), // max 2 (suction not supported)
        "n" => Some(MotorId { device_name: "Lovense Nora", feature_index: 0 }), // nora vibration (needs verification)
        "r" => Some(MotorId { device_name: "Lovense Nora", feature_index: 1 }), // nora rotation (needs verification)
        _ => None
    }
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
fn build_haptic_map(command: &str) -> Result<HashMap<&str, HashMap<u32, f64>>, String> {
    let mut devices: HashMap<&str, HashMap<u32, f64>> = HashMap::new();

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
            Ok(f) => clamp(f),
            Err(e) => return Err(format!("could not parse motor intensity from {}: {:?}", intensity, e))
        };
        match motor_from_tag(tag) {
            Some(motor) => {
                // make a new submap if needed
                devices.entry(motor.device_name)
                    .or_insert(HashMap::new())
                    .insert(motor.feature_index, intensity);
            }
            None => eprintln!("{}: ignoring unknown motor tag {}", LOG_PREFIX_HAPTIC_ENDPOINT, tag)
        };
    };

    // Ok(&mut devices)
    Ok(devices)
}

fn clamp(f: f64) -> f64 {
    if f < 0.0 {
        0.0
    } else if f > 1.0 {
        1.0
    } else {
        f
    }
}

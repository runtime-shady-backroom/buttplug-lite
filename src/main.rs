use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use buttplug::{
    client::{ButtplugClient, ButtplugClientEvent, device::VibrateCommand},
    connector::{ButtplugRemoteClientConnector, ButtplugWebsocketClientTransport},
    core::messages::serializer::ButtplugClientJSONSerializer,
};
use futures::StreamExt;
use tokio::sync::{Mutex, RwLock};
use warp::Filter;
use warp::http::{Response, StatusCode};

// global state types
type WatchdogTimeoutDb = Arc<Mutex<Option<i64>>>;
type HapticClientDb = Arc<RwLock<Option<ButtplugClient>>>;

const EMPTY_STRING: String = String::new();

// how often the watchdog runs its check
const WATCHDOG_POLL_INTERVAL_MILLIS: u64 = 1000;

// halt devices after this much time with no command received
const WATCHDOG_TIMEOUT_MILLIS: i64 = 10000;

// how long to wait before attempting a reconnect to the server
const HAPTIC_SERVER_RECONNECT_DELAY_MILLIS: u64 = 5000;

// name of this client from intiface's perspective
const HAPTIC_SERVER_CLIENT_NAME: &str = "intiface-proxy";

// intiface url
const HAPTIC_SERVER_ADDRESS: &str = "ws://127.0.0.1:12345";

// log prefixes:
const LOG_PREFIX_HAPTIC_ENDPOINT: &str = "/haptic";
const LOG_PREFIX_HAPTIC_SERVER: &str = "haptic_server";

// encodes the "address" of a specific motor
struct MotorId<'a> {
    device_name: &'a str,
    feature_index: u32,
}

#[tokio::main]
async fn main() {
    println!("Initializing {} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    let proxy_server_address: SocketAddr = ([127, 0, 0, 1], 3031).into();

    let haptic_watchdog_db: WatchdogTimeoutDb = Arc::new(Mutex::new(None));
    let haptic_client_db: HapticClientDb = Arc::new(RwLock::new(None));

    // GET /hapticstatus => 200 OK with body containing haptic status
    let hapticstatus = warp::path("hapticstatus")
        .and(warp::get())
        .and(with_db(haptic_client_db.clone()))
        .and_then(haptic_status_handler);

    // POST /hapticscan => 200 OK with empty body
    let hapticscan = warp::path("hapticscan")
        .and(warp::post())
        .and(warp::body::content_length_limit(0))
        .and(with_db(haptic_client_db.clone()))
        .and_then(haptic_scan_handler);

    // WEBSOCKET /haptic
    let haptic = warp::path("haptic")
        .and(warp::ws())
        .and(with_db(haptic_client_db.clone()))
        .and(with_db(haptic_watchdog_db.clone()))
        .map(|ws: warp::ws::Ws, haptic_client: HapticClientDb, watchdog_timestamp: WatchdogTimeoutDb| {
            ws.on_upgrade(|ws| haptic_handler(ws, haptic_client, watchdog_timestamp))
        });

    let routes = hapticstatus
        .or(hapticscan)
        .or(haptic);

    // this is needed in both the watchdog and reconnect background tasks,
    // hence the explicit clone here for use in the watchdog task
    let watchdog_haptic_client_clone = haptic_client_db.clone();

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
                let haptic_client_mutex = watchdog_haptic_client_clone.read().await;
                match haptic_client_mutex.as_ref() {
                    Some(haptic_client) => {
                        match haptic_client.stop_all_devices().await {
                            Ok(()) => (),
                            Err(e) => eprintln!("watchdog: error halting devices: {:?}", e)
                        }
                    }
                    None => () // do nothing because there is no server connected
                }
            }
        }
    });

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
            connect_to_haptic_server(haptic_client_db.clone()).await; // will "block" until disconnect
            tokio::time::sleep(Duration::from_millis(HAPTIC_SERVER_RECONNECT_DELAY_MILLIS)).await; // reconnect delay
        }
    });

    println!("Starting web server");
    warp::serve(routes)
        .run(proxy_server_address) // sacrifice the main thread to warp
        .await;
}

// connect to an intiface server, then while connected process events
// returns only when we disconnect from the server
async fn connect_to_haptic_server(haptic_client_db: HapticClientDb) {
    let mut haptic_client_mutex = haptic_client_db.write().await;
    let haptic_connector = ButtplugRemoteClientConnector::<
        ButtplugWebsocketClientTransport,
        ButtplugClientJSONSerializer,
    >::new(ButtplugWebsocketClientTransport::new_insecure_connector(
        HAPTIC_SERVER_ADDRESS,
    ));
    let haptic_client = ButtplugClient::new(HAPTIC_SERVER_CLIENT_NAME);
    match haptic_client.connect(haptic_connector).await {
        Ok(()) => {
            println!("{}: Intiface connected!", LOG_PREFIX_HAPTIC_SERVER);
            let mut event_stream = haptic_client.event_stream();
            match haptic_client.start_scanning().await {
                Ok(()) => println!("{}: starting device scan", LOG_PREFIX_HAPTIC_SERVER),
                Err(e) => eprintln!("{}: scan failure: {:?}", LOG_PREFIX_HAPTIC_SERVER, e)
            };
            *haptic_client_mutex = Some(haptic_client);
            drop(haptic_client_mutex); // prevent this section from requiring two locks
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
                            let mut haptic_client_mutex = haptic_client_db.write().await;
                            *haptic_client_mutex = None; // not strictly required but will give more sane error messages
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
async fn haptic_status_handler(haptic_client: HapticClientDb) -> Result<impl warp::Reply, warp::Rejection> {
    let haptic_client_mutex = haptic_client.read().await;
    match haptic_client_mutex.as_ref() {
        Some(haptic_client) => {
            let connected = haptic_client.connected();
            let mut string = String::from(format!("intiface connected={}", connected));
            for device in haptic_client.devices() {
                string.push_str(format!("\n  {}", device.name).as_str());
                for (message_type, attributes) in device.allowed_messages.iter() {
                    string.push_str(format!("\n    {:?}: {:?}", message_type, attributes).as_str());
                }
            }
            Ok(string)
        }
        None => Ok(String::from("intiface connected=None"))
    }
}

// trigger a device scan
async fn haptic_scan_handler(haptic_client: HapticClientDb) -> Result<impl warp::Reply, warp::Rejection> {
    let haptic_client_mutex = haptic_client.read().await;
    match haptic_client_mutex.as_ref() {
        Some(haptic_client) => {
            match haptic_client.start_scanning().await {
                Ok(()) => Ok(Response::builder().status(StatusCode::OK).body(EMPTY_STRING)),
                Err(e) => Ok(Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(format!("{:?}", e)))
            }
        }
        None => Ok(Response::builder().status(StatusCode::SERVICE_UNAVAILABLE).body(format!("intiface not connected")))
    }
}

// haptic websocket handler
async fn haptic_handler(websocket: warp::ws::WebSocket, haptic_client: HapticClientDb, watchdog_time: WatchdogTimeoutDb) {
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

        let haptic_client_mutex = haptic_client.read().await;
        match haptic_client_mutex.as_ref() {
            Some(haptic_client) => {
                for device in haptic_client.devices() {
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
                drop(haptic_client_mutex); // prevent this section from requiring two locks

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
            Ok(f) => f,
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

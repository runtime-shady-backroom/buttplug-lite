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
use tokio::sync::Mutex;
use warp::Filter;
use warp::http::{Response, StatusCode};

type IntegerDb = Arc<Mutex<Option<i64>>>;
type HapticClientDb = Arc<Mutex<Option<ButtplugClient>>>;

const EMPTY_STRING: String = String::new();
const WATCHDOG_TIMEOUT_MILLIS: i64 = 10000;
const HAPTIC_SERVER_CLIENT_NAME: &str = "intiface-proxy";
const HAPTIC_SERVER_ADDRESS: &str = "ws://127.0.0.1:12345";

const LOG_PREFIX_HAPTIC_ENDPOINT: &str = "/haptic";
const LOG_PREFIX_HAPTIC_SERVER: &str = "haptic_server";


struct MotorId<'a> {
    name: &'a str,
    feature_index: u32,
}

#[tokio::main]
async fn main() {
    let proxy_server_address: SocketAddr = ([127, 0, 0, 1], 3031).into();

    let haptic_watchdog_db: IntegerDb = Arc::new(Mutex::new(None));
    let haptic_client_db: HapticClientDb = Arc::new(Mutex::new(None));

    // GET /hapticstatus => 200 OK with body containing haptic status
    let hapticstatus = warp::path("hapticstatus")
        .and(warp::get())
        .and(with_haptic_db(haptic_client_db.clone()))
        .and_then(haptic_status_handler);

    // POST /hapticscan => 200 OK with empty body
    let hapticscan = warp::path("hapticscan")
        .and(warp::post())
        .and(warp::body::content_length_limit(0))
        .and(with_haptic_db(haptic_client_db.clone()))
        .and_then(haptic_scan_handler);

    // WEBSOCKET /haptic
    let haptic = warp::path("haptic")
        .and(warp::ws())
        .and(with_haptic_db(haptic_client_db.clone()))
        .and(with_int_db(haptic_watchdog_db.clone()))
        .map(|ws: warp::ws::Ws, haptic_client: HapticClientDb, watchdog_timestamp: IntegerDb| {
            ws.on_upgrade(|ws| haptic_handler(ws, haptic_client, watchdog_timestamp))
        });

    let routes = hapticstatus
        .or(hapticscan)
        .or(haptic);

    let watchdog_haptic_client_clone = haptic_client_db.clone();
    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let unix_time = SystemTime::now().duration_since(UNIX_EPOCH)
                .expect("Your system clock is wrong")
                .as_millis();
            let unix_time = i64::try_from(unix_time).expect("System time out of range");

            let mut watchdog_time_mutex = haptic_watchdog_db.lock().await;
            let watchdog_violation = match *watchdog_time_mutex {
                Some(watchdog_time) => unix_time - watchdog_time > WATCHDOG_TIMEOUT_MILLIS,
                None => false
            };
            if watchdog_violation {
                println!("Watchdog violation! Halting all devices. To avoid this send an update at least every {}ms.", WATCHDOG_TIMEOUT_MILLIS);
                *watchdog_time_mutex = None; // this prevents the message from spamming
                drop(watchdog_time_mutex);
                let haptic_client_mutex = watchdog_haptic_client_clone.lock().await;
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
            connect_to_haptic_server(haptic_client_db.clone()).await; // will "block" until disconnect
            tokio::time::delay_for(Duration::from_secs(5)).await; // reconnect delay
        }
    });

    println!("Starting web server...");
    warp::serve(routes)
        .run(proxy_server_address)
        .await;
}

async fn connect_to_haptic_server(haptic_client_db: HapticClientDb) {
    let mut haptic_client_mutex = haptic_client_db.lock().await;
    let haptic_client = ButtplugRemoteClientConnector::<
        ButtplugWebsocketClientTransport,
        ButtplugClientJSONSerializer,
    >::new(ButtplugWebsocketClientTransport::new_insecure_connector(
        HAPTIC_SERVER_ADDRESS,
    ));
    match ButtplugClient::connect(HAPTIC_SERVER_CLIENT_NAME, haptic_client).await {
        Ok((haptic_client, mut haptic_events)) => {
            println!("{}: Intiface connected!", LOG_PREFIX_HAPTIC_SERVER);
            match haptic_client.start_scanning().await {
                Ok(()) => println!("{}: Scanning for devices...", LOG_PREFIX_HAPTIC_SERVER),
                Err(e) => eprintln!("{}: Scan failure: {:?}", LOG_PREFIX_HAPTIC_SERVER, e)
            };
            *haptic_client_mutex = Some(haptic_client);
            drop(haptic_client_mutex);
            loop {
                println!("{}: waiting for event...", LOG_PREFIX_HAPTIC_SERVER); //TODO: remove when done debugging deadlock
                match haptic_events.next().await {
                    Some(event) => match event {
                        ButtplugClientEvent::DeviceAdded(dev) => println!("{}: device connected: {}", LOG_PREFIX_HAPTIC_SERVER, dev.name),
                        ButtplugClientEvent::DeviceRemoved(dev) => println!("{}: device disconnected: {}", LOG_PREFIX_HAPTIC_SERVER, dev.name),
                        ButtplugClientEvent::PingTimeout => println!("{}: ping timeout", LOG_PREFIX_HAPTIC_SERVER),
                        ButtplugClientEvent::Error(e) => println!("{}: server error: {:?}", LOG_PREFIX_HAPTIC_SERVER, e),
                        ButtplugClientEvent::ScanningFinished => println!("{}: scan finished", LOG_PREFIX_HAPTIC_SERVER),
                        ButtplugClientEvent::ServerDisconnect => {
                            println!("{}: server disconnected", LOG_PREFIX_HAPTIC_SERVER);
                            let mut haptic_client_mutex = haptic_client_db.lock().await;
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

fn with_int_db(db: IntegerDb) -> impl Filter<Extract=(IntegerDb, ), Error=std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

fn with_haptic_db(db: HapticClientDb) -> impl Filter<Extract=(HapticClientDb, ), Error=std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

async fn haptic_status_handler(haptic_client: HapticClientDb) -> Result<impl warp::Reply, warp::Rejection> {
    let haptic_client_mutex = haptic_client.lock().await;
    match haptic_client_mutex.as_ref() {
        Some(haptic_client) => {
            let connected = haptic_client.connected();
            let mut string = String::from(format!("intiface connected={}", connected));
            for device in haptic_client.devices() {
                string.push_str(format!("\n  {}", device.name).as_str());
                for (message_type, attributes) in device.allowed_messages {
                    string.push_str(format!("\n    {:?}: {:?}", message_type, attributes).as_str());
                }
            }
            Ok(string)
        }
        None => Ok(String::from("intiface connected=None"))
    }
}

async fn haptic_scan_handler(haptic_client: HapticClientDb) -> Result<impl warp::Reply, warp::Rejection> {
    let haptic_client_mutex = haptic_client.lock().await;
    match haptic_client_mutex.as_ref() {
        Some(haptic_client) => {
            match haptic_client.start_scanning().await {
                Ok(()) => Ok(Response::builder().status(StatusCode::OK).body(EMPTY_STRING)),
                Err(e) => Ok(Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(format!("{:?}", e)))
            }
        }
        None => Ok(Response::builder().status(StatusCode::SERVICE_UNAVAILABLE).body(format!("intifiace not connected")))
    }
}

// haptic handler
async fn haptic_handler(websocket: warp::ws::WebSocket, haptic_client: HapticClientDb, watchdog_time: IntegerDb) {
    let (_, mut rx) = websocket.split();
    while let Some(result) = rx.next().await {
        let message = match result {
            Ok(message) => message,
            Err(e) => {
                eprintln!("{}: message error: {:?}", LOG_PREFIX_HAPTIC_ENDPOINT, e);
                break;
            }
        };
        let message = match message.to_str() {
            Ok(str) => str,
            Err(_) => {
                if message.is_binary() || message.is_close() {
                    eprintln!("{}: error converting message to string: {:?}", LOG_PREFIX_HAPTIC_ENDPOINT, message);
                } else if message.is_close() {
                    println!("{}: client closed connection", LOG_PREFIX_HAPTIC_ENDPOINT)
                }
                // no need to log ping or pong messages
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

        let haptic_client_mutex = haptic_client.lock().await;
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
                drop(haptic_client_mutex);

                // feed the watchdog
                let mut watchdog_time_mutex = watchdog_time.lock().await;
                let unix_time = SystemTime::now().duration_since(UNIX_EPOCH)
                    .expect("Your system clock is wrong")
                    .as_millis();
                *watchdog_time_mutex = Some(i64::try_from(unix_time).expect("System time out of range"));
            }
            None => () // no server connected, so send no commands
        }
    }
    println!("{}: disconnected", LOG_PREFIX_HAPTIC_ENDPOINT);
}

fn motor_from_tag<'a>(tag: &str) -> Option<MotorId<'a>> {
    match tag {
        "o" => Some(MotorId { name: "Lovense Edge", feature_index: 0 }), // edge outer (verified)
        "i" => Some(MotorId { name: "Lovense Edge", feature_index: 1 }), // edge inner (verified)
        "h" => Some(MotorId { name: "Lovense Hush", feature_index: 0 }), // hush
        "l" => Some(MotorId { name: "Lovense Lush", feature_index: 0 }), // lush 2
        "m" => Some(MotorId { name: "Lovense Max", feature_index: 0 }), // max 2 (suction not supported)
        "n" => Some(MotorId { name: "Lovense Nora", feature_index: 0 }), // nora vibration (needs verification)
        "r" => Some(MotorId { name: "Lovense Nora", feature_index: 1 }), // nora rotation (needs verification)
        _ => None
    }
}

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
                devices.entry(motor.name)
                    .or_insert(HashMap::new())
                    .insert(motor.feature_index, intensity);
            }
            None => eprintln!("Ignoring unknown motor tag {}", tag)
        };
    };

    // Ok(&mut devices)
    Ok(devices)
}

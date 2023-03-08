// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use std::collections::HashMap;
use std::convert;
use std::net::SocketAddr;

use buttplug::client::{LinearCommand, RotateCommand, ScalarCommand};
use buttplug::core::message::ButtplugDeviceMessageType;
use futures::StreamExt;
use tokio::sync::{mpsc, oneshot};
use tokio::task;
use tracing::{debug, error, info, warn};
use warp::Filter;

use crate::app::structs::{ApplicationStateDb, MotorSettings};
use crate::app::webserver::shutdown_message::ShutdownMessage;
use crate::config::v3::{ConfigurationV3, MotorTypeV3};
use crate::util::extensions::FloatExtensions;
use crate::util::watchdog;
use crate::util::watchdog::WatchdogTimeoutDb;

static LOG_PREFIX_HAPTIC_ENDPOINT: &str = "/haptic";

pub fn start_webserver(
    application_state_db: ApplicationStateDb,
    watchdog_timeout_db: WatchdogTimeoutDb,
    initial_config_loaded_rx: oneshot::Receiver<()>,
    gui_start_tx: oneshot::Sender<()>,
    mut warp_shutdown_initiate_rx: mpsc::UnboundedReceiver<ShutdownMessage>,
    warp_shutdown_complete_tx: oneshot::Sender<()>,

) {
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

    // moved into the following task
    let reconnect_task_application_state_db_clone = application_state_db.clone();
    task::spawn(async move {
        initial_config_loaded_rx.await.expect("failed to load initial configuration");

        let mut gui_start_oneshot_tx = Some(gui_start_tx); // will get None'd after the first loop

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
}

fn with_db<T: Clone + Send>(db: T) -> impl Filter<Extract=(T, ), Error=convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
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

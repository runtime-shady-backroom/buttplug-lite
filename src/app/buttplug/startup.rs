// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! The buttplug server startup code is so huge I'm putting it in its own file

use std::ops::DerefMut as _;
use std::time::Duration;

use buttplug::client::{ButtplugClient, ButtplugClientEvent};
use buttplug::core::connector::ButtplugInProcessClientConnectorBuilder;
use buttplug::server::ButtplugServerBuilder;
use buttplug::server::device::configuration::DeviceConfigurationManagerBuilder;
use buttplug::server::device::hardware::communication::{
    btleplug::BtlePlugCommunicationManagerBuilder,
    lovense_connect_service::LovenseConnectServiceCommunicationManagerBuilder,
    lovense_dongle::LovenseHIDDongleCommunicationManagerBuilder,
    lovense_dongle::LovenseSerialDongleCommunicationManagerBuilder,
    serialport::SerialPortCommunicationManagerBuilder,
};
use buttplug::server::device::ServerDeviceManagerBuilder;
use futures::StreamExt as _;
use tokio::sync::{mpsc, oneshot};
use tokio::task;
use tracing::{info, warn};

use crate::app::buttplug::functions::debug_name_from_device;
use crate::app::structs::{ApplicationState, ApplicationStateDb};
use crate::config;
use crate::gui::subscription::ApplicationStatusEvent;

// how long to wait before attempting a reconnect to the server
const BUTTPLUG_SERVER_RECONNECT_DELAY_MILLIS: u64 = 5000;

// log prefixes:
static LOG_PREFIX_BUTTPLUG_SERVER: &str = "buttplug_server";

// name of this client from the buttplug.io server's perspective
static BUTTPLUG_CLIENT_NAME: &str = "in-process-client";

pub async fn start_server(
    application_state: ApplicationStateDb,
    initial_config_loaded_tx: oneshot::Sender<()>,
    application_status_sender: mpsc::UnboundedSender<ApplicationStatusEvent>,
) {
    let mut initial_config_loaded_tx = Some(initial_config_loaded_tx);

    // spawn the server reconnect task
    // when the server is connected this functions as the event reader
    // when the server is disconnected it attempts to reconnect after a delay
    task::spawn(async move {
        loop {
            // we reconnect here regardless of server state
            start_server_internal(application_state.clone(), initial_config_loaded_tx, application_status_sender.clone()).await; // will "block" until disconnect
            initial_config_loaded_tx = None; // only Some() for the first loop
            tokio::time::sleep(Duration::from_millis(BUTTPLUG_SERVER_RECONNECT_DELAY_MILLIS)).await; // reconnect delay
        }
    });
}

// start server, then while running process events
// returns only when we disconnect from the server
async fn start_server_internal(
    application_state_db: ApplicationStateDb,
    initial_config_loaded_tx: Option<oneshot::Sender<()>>,
    application_status_event_sender: mpsc::UnboundedSender<ApplicationStatusEvent>,
) {
    let mut application_state_mutex = application_state_db.write().await;
    let buttplug_client = ButtplugClient::new(BUTTPLUG_CLIENT_NAME);

    // buttplug::util::in_process_client has a good example of how to do this, and so does https://github.com/buttplugio/docs.buttplug.io/blob/master/examples/rust/src/bin/embedded_connector.rs
    let device_configuration_manager = DeviceConfigurationManagerBuilder::default()
        .allow_raw_messages(false)
        .finish()
        .unwrap();
    let mut device_manager_builder = ServerDeviceManagerBuilder::new(device_configuration_manager);
    device_manager_builder
        .comm_manager(BtlePlugCommunicationManagerBuilder::default())
        .comm_manager(SerialPortCommunicationManagerBuilder::default())
        .comm_manager(LovenseHIDDongleCommunicationManagerBuilder::default())
        .comm_manager(LovenseSerialDongleCommunicationManagerBuilder::default())
        .comm_manager(LovenseConnectServiceCommunicationManagerBuilder::default());

    #[cfg(target_os = "windows")] {
        use buttplug::server::device::hardware::communication::xinput::XInputDeviceCommunicationManagerBuilder;
        device_manager_builder.comm_manager(XInputDeviceCommunicationManagerBuilder::default());
    }

    let server = ButtplugServerBuilder::new(device_manager_builder.finish().unwrap())
        .name("buttplug-lite")
        .finish()
        .expect("Failed to initialize buttplug server");

    /* We're allowed to steal a reference to this…
     * and we're going to use it to get unique device IDs for duplicate device detection.
     * This is absolutely an evil hack but I have ZERO idea how else I'm supposed to do this
     * while using the ButtplugInProcessClientConnector, because the connector completely consumes
     * the ButtplugServer struct.
     */
    let device_manager = server.device_manager();

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
                Some(ApplicationState { configuration, .. }) => configuration,
                None => {
                    config::load_configuration().await
                }
            };

            *application_state_mutex = Some(ApplicationState { client: buttplug_client, configuration, device_manager: device_manager.clone() });
            drop(application_state_mutex); // prevent this section from requiring two locks

            if let Some(sender) = initial_config_loaded_tx {
                sender.send(()).expect("failed to send config-loaded signal");
            }

            loop {
                match event_stream.next().await {
                    Some(event) => match event {
                        ButtplugClientEvent::DeviceAdded(dev) => {
                            info!("{LOG_PREFIX_BUTTPLUG_SERVER}: device connected: {}", debug_name_from_device(&dev, &device_manager));
                            application_status_event_sender.send(ApplicationStatusEvent::DeviceAdded).expect("failed to send device added event");
                        }
                        ButtplugClientEvent::DeviceRemoved(dev) => {
                            info!("{LOG_PREFIX_BUTTPLUG_SERVER}: device disconnected: {}", debug_name_from_device(&dev, &device_manager));
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
        Err(e) => warn!("{LOG_PREFIX_BUTTPLUG_SERVER}: failed to connect to server. Will retry shortly… ({e:?})") // will try to reconnect later, may not need to log this error
    }
}

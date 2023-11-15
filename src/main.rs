// Copyright 2022-2023 runtime-shady-backroom and buttplug-lite contributors.
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

// necessary to remove the weird console window that appears alongside the real GUI on Windows
#![windows_subsystem = "windows"]

use std::ops::DerefMut as _;
use std::process;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::time::Duration;

use clap::Parser as _;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::task;
use tracing::{info, warn};

use crate::app::buttplug;
use crate::app::structs::{ApplicationState, ApplicationStateDb, CliArgs};
use crate::app::webserver::ShutdownMessage;
use crate::gui::subscription::{ApplicationStatusEvent, SubscriptionProvider};
use crate::util::{logging, watchdog};
use crate::util::exfiltrator::ServerDeviceIdentifier;
use crate::util::watchdog::WatchdogTimeoutDb;

mod app;
mod config;
mod gui;
mod util;

fn main() {
    util::GLOBAL_TOKIO_RUNTIME.block_on(tokio_main())
}

async fn tokio_main() {
    let args: CliArgs = CliArgs::parse();

    // run self-checks to make sure our unsafe hack to steal private fields appears to be working
    ServerDeviceIdentifier::test();

    if args.self_check {
        process::exit(0);
    }

    // after logging init we can use tracing to log. Any tracing logs before this point go nowhere.
    let _log_guard = logging::init(
        args.verbose,
        args.log_filter,
        args.stdout,
        args.force_panic_handler,
        !args.no_panic_handler
    );

    info!("initializing {} {} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), env!("GIT_COMMIT_HASH"));

    let watchdog_timeout_db: WatchdogTimeoutDb = Arc::new(AtomicI64::new(i64::MAX));
    let application_state_db: ApplicationStateDb = Arc::new(RwLock::new(None));

    watchdog::start(watchdog_timeout_db.clone(), application_state_db.clone());

    // used to send initial port over from the configuration load
    let (initial_config_loaded_tx, initial_config_loaded_rx) = oneshot::channel::<()>();
    let (application_status_sender, application_status_receiver) = mpsc::unbounded_channel::<ApplicationStatusEvent>();

    // test ticks
    if let Some(interval) = args.debug_ticks {
        let test_tick_sender = application_status_sender.clone();
        task::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval));
            loop {
                interval.tick().await;
                test_tick_sender.send(ApplicationStatusEvent::next_tick()).expect("WHO DROPPED MY FUCKING RECEIVER? (I wrote this code, so it was me!)");
            }
        });
    }

    buttplug::start_server(application_state_db.clone(), initial_config_loaded_tx, application_status_sender).await;

    // use to shut down or restart the webserver
    let (warp_shutdown_initiate_tx, warp_shutdown_initiate_rx) = mpsc::unbounded_channel::<ShutdownMessage>();

    // called once warp is done dying
    let (warp_shutdown_complete_tx, warp_shutdown_complete_rx) = oneshot::channel::<()>();

    // triggers the GUI to start, only called after warp spins up
    let (gui_start_tx, gui_start_rx) = oneshot::channel::<()>();

    // start up the webserver
    app::webserver::start_webserver(
        application_state_db.clone(),
        watchdog_timeout_db,
        initial_config_loaded_rx,
        gui_start_tx,
        warp_shutdown_initiate_rx,
        warp_shutdown_complete_tx,
    );

    if let Ok(()) = gui_start_rx.await {
        //TODO: wait for buttplug to notice devices
        let initial_devices = buttplug::get_tagged_devices(&application_state_db).await.expect("Application failed to initialize");

        let subscription = SubscriptionProvider::new(application_status_receiver);
        gui::run(application_state_db.clone(), warp_shutdown_initiate_tx, initial_devices, subscription); // blocking call

        // NOTE: iced hard kills the application when the windows is closed!
        // That means this code is unreachable.
        // As far as I'm aware it is currently impossible to register any sort of shutdown
        // hook/return/signal from iced once you sacrifice your main thread.
        // For now this is fine, as we don't have any super sensitive shutdown code to run,
        // especially with the buttplug server being (apparently?) unstoppable.
    }

    // at this point we begin cleaning up resources for shutdown
    info!("shutting down…");

    // but first, wait for warp to close
    if let Err(e) = warp_shutdown_complete_rx.await {
        info!("error shutting down warp webserver: {e:?}")
    } else {
        info!("initiated warp webserver graceful shutdown");
    }

    // it's be nice if I could shut down buttplug with `server.shutdown()`, but I'm forced to give server ownership to the connector
    // it'd be nice if I could shut down buttplug with `connector.server_ref().shutdown();`, but I'm forced to give connector ownership to the client
    let mut application_state_mutex = application_state_db.write().await;
    if let Some(application_state) = application_state_mutex.deref_mut() {
        if let Err(e) = application_state.client.disconnect().await {
            warn!("Unable to disconnect internal client from internal server: {e}");
        }
    }

    info!("shutdown complete");
}

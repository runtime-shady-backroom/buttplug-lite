use std::convert::TryFrom;
use std::ops::Add;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use tokio::task;

use crate::ApplicationStateDb;

pub type WatchdogTimeoutDb = Arc<AtomicI64>;

// how often the watchdog runs its check
const WATCHDOG_POLL_INTERVAL_MILLIS: u64 = 1000;

// halt devices after this much time with no command received
const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(10);

pub fn start(watchdog_timeout_db: WatchdogTimeoutDb, buttplug_connector_db: ApplicationStateDb) {
    // spawn the watchdog task
    // if too much time passes with no input from the client, this halts all haptic devices
    task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(WATCHDOG_POLL_INTERVAL_MILLIS));
        loop {
            interval.tick().await;
            let watchdog_violation = unix_time() > watchdog_timeout_db.load(Ordering::Relaxed);
            if watchdog_violation {
                println!("Watchdog violation! Halting all devices. To avoid this send an update at least every {}ms.", WATCHDOG_TIMEOUT.as_millis());
                watchdog_timeout_db.store(i64::MAX, Ordering::Relaxed); // this prevents the message from spamming
                let buttplug_connector_mutex = buttplug_connector_db.read().await;
                if let Some(buttplug_connector) = buttplug_connector_mutex.as_ref() {
                    match buttplug_connector.client.stop_all_devices().await {
                        Ok(()) => (),
                        Err(e) => eprintln!("watchdog: error halting devices: {:?}", e)
                    }
                } // else, do nothing because there is no server connected
            }
        }
    });
}

/// feed the watchdog, preventing it from kicking in for WATCHDOG_TIMEOUT more time
pub async fn feed(watchdog_timeout_db: &WatchdogTimeoutDb) {
    watchdog_timeout_db.store(calculate_timeout(), Ordering::Relaxed);
}

fn unix_time_plus(plus: Duration) -> i64 {
    let unix_time = UNIX_EPOCH.elapsed()
        .expect("Your system clock is wrong")
        .add(plus)
        .as_millis();

    // probably fine to panic if your system clock is before the unix epoch...
    i64::try_from(unix_time).expect("System time out of range")
}

fn calculate_timeout() -> i64 {
    unix_time_plus(WATCHDOG_TIMEOUT)
}

fn unix_time() -> i64 {
    unix_time_plus(Duration::from_secs(0))
}

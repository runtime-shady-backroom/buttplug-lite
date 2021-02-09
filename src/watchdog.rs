use std::convert::TryFrom;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;
use tokio::task;

use crate::HapticConnectorDb;

pub type WatchdogTimeoutDb = Arc<Mutex<Option<i64>>>;

// how often the watchdog runs its check
const WATCHDOG_POLL_INTERVAL_MILLIS: u64 = 1000;

// halt devices after this much time with no command received
const WATCHDOG_TIMEOUT_MILLIS: i64 = 10000;

pub fn start(watchdog_timeout_db: WatchdogTimeoutDb, haptic_connector_db: HapticConnectorDb) {
    // spawn the watchdog task
    // if too much time passes with no input from the client, this halts all haptic devices
    task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(WATCHDOG_POLL_INTERVAL_MILLIS));
        loop {
            interval.tick().await;
            let unix_time = SystemTime::now().duration_since(UNIX_EPOCH)
                .expect("Your system clock is wrong")
                .as_millis();
            let unix_time = i64::try_from(unix_time).expect("System time out of range");
            // probably fine to panic if your system clock is before the unix epoch...

            let mut watchdog_time_mutex = watchdog_timeout_db.lock().await;
            let watchdog_violation = match *watchdog_time_mutex {
                Some(watchdog_time) => unix_time - watchdog_time > WATCHDOG_TIMEOUT_MILLIS,
                None => false
            };
            if watchdog_violation {
                println!("Watchdog violation! Halting all devices. To avoid this send an update at least every {}ms.", WATCHDOG_TIMEOUT_MILLIS);
                *watchdog_time_mutex = None; // this prevents the message from spamming
                drop(watchdog_time_mutex); // prevent this section from requiring two locks
                let haptic_connector_mutex = haptic_connector_db.read().await;
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
}

pub async fn feed(watchdog_timeout_db: &WatchdogTimeoutDb) {
    // feed the watchdog
    let mut watchdog_time_mutex = watchdog_timeout_db.lock().await;
    let unix_time = SystemTime::now().duration_since(UNIX_EPOCH)
        .expect("Your system clock is wrong")
        .as_millis();
    *watchdog_time_mutex = Some(i64::try_from(unix_time).expect("System time out of range"));
    // probably fine to panic if your system clock is before the unix epoch...
}

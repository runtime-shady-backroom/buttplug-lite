// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! GitHub Releases-based update checking

use std::cmp::Ordering;
use std::time::Duration;

use semver::Version;
use serde::Deserialize;
use tracing::{info, warn};

const USER_AGENT: &str = "runtime-shady-backroom/buttplug-lite";
const UPDATE_CHECK_URI: &str = "https://api.github.com/repos/runtime-shady-backroom/buttplug-lite/releases/latest";

/// Compare the local version to the latest GitHub release. If there's a newer version available, return its URL.
pub async fn check_for_update(local_version: Version) -> Option<String> {

    match get_latest_release().await {
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
                            warn!("Local version is NEWER than remote version! If you're not beta testing a pre-release then something has gone terribly wrong.");
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
                    warn!("Error parsing remote version: {e:?}");
                    None
                }
            }
        }
        Err(e) => {
            warn!("Failed to get latest version info: {e:?}");
            None
        }
    }
}

/// Get latest release from GitHub
async fn get_latest_release() -> Result<GithubRelease, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let request = client.get(UPDATE_CHECK_URI)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to build update check HTTP request: {e}"))?;

    let response = client.execute(request).await
        .map_err(|e| format!("Update check failed: {e}"))?;

    // note that this buffers the entire JSON response before it begins parsing
    response.json::<GithubRelease>().await
        .map_err(|e| format!("error parsing github release response body: {e}"))
}

/// GitHub API response object
#[derive(Deserialize)]
struct GithubRelease {
    pub html_url: String,
    pub tag_name: String,
}

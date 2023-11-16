// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! GitHub Releases-based update checking

use std::cmp::Ordering;
use std::time::Duration;
use reqwest::header;

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
        .gzip(true)
        .http2_prior_knowledge()
        .https_only(true)
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(3))
        .connection_verbose(true)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let request = client.get(UPDATE_CHECK_URI)
        .header(header::ACCEPT, "application/json")
        .build()
        .map_err(|e| format!("Failed to build update check HTTP request: {e}"))?;

    let response = client.execute(request).await
        .map_err(|e| format!("Update check failed: {e}"))?;

    // Note that this buffers the entire JSON response before it begins parsing.
    //
    // This is honestly fine for a few reasons:
    // 1. The response body is only like 2kb, and it won't get larger over time.
    // 2. serde_json doesn't have great tech for streaming over response data
    // 3. using a Reader on some sort of response data buffer involves copies
    // 4. we do this a grand total of once and it's in the background
    //
    // If I really care about optimizing this, the _real_ target would be to implement etag-based caching.
    let status = response.status();
    response.json::<GithubRelease>().await
        .map_err(|e| format!("error parsing github release {} response: {}", status.as_str(), e))
}

/// GitHub API response object
#[derive(Deserialize)]
struct GithubRelease {
    pub html_url: String,
    pub tag_name: String,
}

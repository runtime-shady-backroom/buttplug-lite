// Copyright 2022-2023 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

//! GitHub Releases-based update checking

use std::cmp::Ordering;
use std::time::Duration;

use bytes::Buf as _;
use hyper::{self, Body, Client, Method, Request, Response, Uri};
use hyper_timeout::TimeoutConnector;
use hyper_tls::HttpsConnector;
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
    let uri: Uri = UPDATE_CHECK_URI.parse().map_err(|e| format!("update check URI failed to parse: {e:?}"))?;
    let connector = HttpsConnector::new();
    let mut connector = TimeoutConnector::new(connector);

    // We wait for this to complete before we open the GUI, so we MUST have a short timeout.
    connector.set_connect_timeout(Some(Duration::from_secs(10)));
    connector.set_read_timeout(Some(Duration::from_secs(3)));
    connector.set_write_timeout(Some(Duration::from_secs(3)));

    let client = Client::builder()
        .build::<_, Body>(connector);

    let request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(hyper::header::USER_AGENT, USER_AGENT)
        .body(Body::empty())
        .expect("failed to build github release request");

    let response: Response<Body> = client.request(request).await
        .map_err(|e| format!("Could not read github release response body: {e:?}"))?;
    deserialize_response(response).await
}

/// deserialize a GitHub API response
async fn deserialize_response(response: Response<Body>) -> Result<GithubRelease, String> {
    let body = hyper::body::aggregate(response).await
        .map_err(|e| format!("error aggregating github release response body: {e:?}"))?;
    serde_json::from_reader(body.reader())
        .map_err(|e| format!("error parsing github release response body: {e:?}"))
}

/// GitHub API response object
#[derive(Deserialize)]
struct GithubRelease {
    pub html_url: String,
    pub tag_name: String,
}

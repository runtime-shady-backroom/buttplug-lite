// Copyright 2022 runtime-shady-backroom
// This file is part of buttplug-lite.
// buttplug-lite is licensed under the AGPL-3.0 license (see LICENSE file for details).

use std::time::Duration;

use bytes::Buf as _;
use hyper::{self, Body, Client, Method, Request, Response, Uri};
use hyper_timeout::TimeoutConnector;
use hyper_tls::HttpsConnector;
use serde::Deserialize;

const UPDATE_CHECK_URI: &str = "https://api.github.com/repos/runtime-shady-backroom/buttplug-lite/releases/latest";

pub async fn check_for_update() -> Result<GithubRelease, String> {
    let uri: Uri = UPDATE_CHECK_URI.parse().map_err(|e| format!("update check URI failed to parse: {e:?}"))?;
    let connector = HttpsConnector::new();
    let mut connector = TimeoutConnector::new(connector);
    connector.set_connect_timeout(Some(Duration::from_secs(3)));
    connector.set_read_timeout(Some(Duration::from_secs(1)));
    connector.set_write_timeout(Some(Duration::from_secs(1)));

    let client = Client::builder()
        .build::<_, Body>(connector);

    let request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(hyper::header::USER_AGENT, "runtime-shady-backroom/buttplug-lite")
        .body(Body::empty())
        .expect("failed to build github release request");

    let response: Response<Body> = client.request(request).await
        .map_err(|e| format!("Could not read github release response body: {e:?}"))?;
    deserialize_response(response).await
}

async fn deserialize_response(response: Response<Body>) -> Result<GithubRelease, String> {
    let body = hyper::body::aggregate(response).await
        .map_err(|e| format!("error aggregating github release response body: {e:?}"))?;
    serde_json::from_reader(body.reader())
        .map_err(|e| format!("error parsing github release response body: {e:?}"))
}

#[derive(Deserialize)]
pub struct GithubRelease {
    pub html_url: String,
    pub tag_name: String,
}

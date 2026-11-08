//! A daily "this install is active" count through Google Analytics' Measurement Protocol, so we
//! know how many people use Dagpeek. It sends a random install id, the app version and the OS,
//! and nothing about your servers, DAGs or runs. Settings turns it off.

use std::time::Duration;

const MEASUREMENT_ID: &str = "G-GQ102DTLE4";

pub async fn send_active(api_secret: &str, install_id: &str, version: &str, os: &str, day: &str) -> Result<(), String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().map_err(|e| e.to_string())?;
    let body = serde_json::json!({
        "client_id": install_id,
        "non_personalized_ads": true,
        "events": [{
            "name": "app_active",
            "params": { "app_version": version, "operating_system": os, "session_id": day, "engagement_time_msec": 1 }
        }]
    });
    let response = client
        .post("https://www.google-analytics.com/mp/collect")
        .query(&[("measurement_id", MEASUREMENT_ID), ("api_secret", api_secret)])
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("the analytics endpoint answered {}", response.status()))
    }
}

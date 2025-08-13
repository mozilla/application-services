/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::sync::LazyLock;

use crate::error::{ComponentError, EmitTelemetryError};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use url::Url;
use viaduct::Request;

static DEFAULT_TELEMETRY_ENDPOINT: &str = "https://ads.mozilla.org/v1/log";
static TELEMETRY_ENDPONT: LazyLock<RwLock<String>> =
    LazyLock::new(|| RwLock::new(DEFAULT_TELEMETRY_ENDPOINT.to_string()));

#[cfg(test)]
pub fn set_telemetry_endpoint(endpoint: String) {
    let mut telemetry_endpoint_lock = TELEMETRY_ENDPONT.write();
    *telemetry_endpoint_lock = endpoint;
}

fn get_telemetry_endpoint() -> String {
    TELEMETRY_ENDPONT.read().clone()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryEvent {
    Init,
    RenderError,
    AdLoadError,
    FetchError,
    InvalidUrlError,
}

#[allow(dead_code)]
pub trait TrackError<T, ComponentError> {
    fn emit_telemetry_if_error(self) -> Self;
    fn emit_telemetry_if_error_conditionally<F>(self, condition: F) -> Self
    where
        F: Fn(&ComponentError) -> bool;
}

impl<T> TrackError<T, ComponentError> for Result<T, ComponentError> {
    /// Attempts to emit a telemetry event if the Error type can map to an event type.
    fn emit_telemetry_if_error(self) -> Self {
        if let Err(ref err) = self {
            let error_type = map_error_to_event_type(err);
            let _ = emit_telemetry_event(error_type);
        }
        self
    }

    /// Same as `emit_telemetry_if_error` but also requires the given closure `condition` returns true.
    fn emit_telemetry_if_error_conditionally<F>(self, condition: F) -> Self
    where
        F: Fn(&ComponentError) -> bool,
    {
        if let Err(ref err) = self {
            if condition(err) {
                let error_type = map_error_to_event_type(err);
                let _ = emit_telemetry_event(error_type);
            }
        }
        self
    }
}

fn map_error_to_event_type(err: &ComponentError) -> Option<TelemetryEvent> {
    match err {
        ComponentError::RequestAds(_) => Some(TelemetryEvent::FetchError),
        ComponentError::RecordImpression(_) => Some(TelemetryEvent::InvalidUrlError),
        ComponentError::RecordClick(_) => Some(TelemetryEvent::InvalidUrlError),
        ComponentError::ReportAd(_) => Some(TelemetryEvent::InvalidUrlError),
    }
}

pub fn emit_telemetry_event(event_type: Option<TelemetryEvent>) -> Result<(), EmitTelemetryError> {
    let endpoint = get_telemetry_endpoint();
    let mut url = Url::parse(&endpoint)?;
    if let Some(event) = event_type {
        let event_string = serde_json::to_string(&event)?;
        url.set_query(Some(&format!("event={}", event_string)));
        Request::get(url).send()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{CallbackRequestError, ComponentError, RecordClickError};
    use mockito::mock;

    #[test]
    fn test_emit_telemetry_emits_telemetry_for_mappable_error() {
        viaduct_reqwest::use_reqwest_backend();
        set_telemetry_endpoint(format!("{}{}", mockito::server_url(), "/v1/log"));
        let mock = mock("GET", "/v1/log")
            .match_query(mockito::Matcher::UrlEncoded(
                "event".into(),
                "\"invalid_url_error\"".into(),
            ))
            .with_status(200)
            .expect(1)
            .create();

        let result: Result<(), ComponentError> = Err(ComponentError::RecordClick(
            RecordClickError::CallbackRequest(CallbackRequestError::MissingCallback {
                message: "bad url".into(),
            }),
        ));

        let res = result.emit_telemetry_if_error();

        mock.assert();

        assert!(res.is_err());
    }

    #[test]
    fn test_emit_telemetry_conditionally_emits_only_when_condition_met() {
        viaduct_reqwest::use_reqwest_backend();
        set_telemetry_endpoint(format!("{}{}", mockito::server_url(), "/v1/log"));

        let mock_1 = mock("GET", "/v1/log")
            .match_query(mockito::Matcher::UrlEncoded(
                "event".into(),
                "\"invalid_url_error\"".into(),
            ))
            .with_status(200)
            .expect(0)
            .create();

        let result_1: Result<(), ComponentError> = Err(ComponentError::RecordClick(
            RecordClickError::CallbackRequest(CallbackRequestError::MissingCallback {
                message: "bad url".into(),
            }),
        ));
        let res_1 = result_1.emit_telemetry_if_error_conditionally(|_| false);
        mock_1.assert();
        assert!(res_1.is_err());

        let mock_2 = mock("GET", "/v1/log")
            .match_query(mockito::Matcher::UrlEncoded(
                "event".into(),
                "\"invalid_url_error\"".into(),
            ))
            .with_status(200)
            .expect(1)
            .create();

        let result_2: Result<(), ComponentError> = Err(ComponentError::RecordClick(
            RecordClickError::CallbackRequest(CallbackRequestError::MissingCallback {
                message: "bad url".into(),
            }),
        ));
        let res_2 = result_2.emit_telemetry_if_error_conditionally(|_| true);
        mock_2.assert();
        assert!(res_2.is_err());
    }

    #[test]
    fn test_emit_telemetry_event_on_ok_does_nothing() {
        viaduct_reqwest::use_reqwest_backend();
        set_telemetry_endpoint(format!("{}{}", mockito::server_url(), "/v1/log"));

        let mock = mock("GET", "/v1/log").with_status(200).expect(0).create();

        let result: Result<String, ComponentError> =
            Ok("All is good".to_string()).emit_telemetry_if_error();

        mock.assert();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "All is good".to_string());
    }
}

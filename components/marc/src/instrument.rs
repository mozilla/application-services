use crate::error::{ComponentError, EmitTelemetryError};
use serde::{Deserialize, Serialize};
use url::Url;
use viaduct::Request;

const DEFAULT_TELEMETRY_ENDPOINT: &str = "https://ads.allizom.org/v1/log";

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
    fn track(self) -> Self;
    fn track_if<F>(self, condition: F) -> Self
    where
        F: Fn(&ComponentError) -> bool;
}

impl<T> TrackError<T, ComponentError> for Result<T, ComponentError> {
    /// Attempts to emit a telemetry event if the Error type can map to an event type.
    fn track(self) -> Self {
        if let Err(ref err) = self {
            let error_type = map_error_to_event_type(err);
            let _ = emit_telemetry_event(error_type);
        }
        self
    }

    /// Same as `track` but also requires the given closure `condition` returns true.
    fn track_if<F>(self, condition: F) -> Self
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
    let mut url = Url::parse(DEFAULT_TELEMETRY_ENDPOINT)?;
    if let Some(event) = event_type {
        let event_string = serde_json::to_string(&event)?;
        url.set_query(Some(&format!("event={}", event_string)));
        Request::get(url).send()?;
    }
    Ok(())
}

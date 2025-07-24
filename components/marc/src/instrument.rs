use crate::error::{Error, Result};
use url::Url;
use viaduct::Request;

const DEFAULT_TELEMETRY_ENDPOINT: &str = "https://ads.allizom.org/v1/log";

pub trait TrackError<T, E> {
    fn track(self) -> Self;
    fn track_if<F>(self, condition: F) -> Self
    where
        F: Fn(&E) -> bool;
}

// Implementation for Result<T, Error>
impl<T> TrackError<T, Error> for Result<T> {
    fn track(self) -> Self {
        if let Err(ref err) = self {
            let _ = emit_telemetry_event(err);
        }
        self
    }

    fn track_if<F>(self, condition: F) -> Self
    where
        F: Fn(&Error) -> bool,
    {
        if let Err(ref err) = self {
            if condition(err) {
                let _ = emit_telemetry_event(err);
            }
        }
        self
    }
}

enum TelemetryEvent {
    Init,
    RenderError,
    AdLoadError,
    FetchError,
    InvalidUrlError,
}

fn map_error_to_event_type(err: &Error) -> Option<TelemetryEvent> {
    match err {
        Error::UrlParse(_) => return Some(TelemetryEvent::InvalidUrlError),
        Error::Request(_) => return Some(TelemetryEvent::FetchError),
        Error::Json(_) => {}
        Error::Validation { .. } => {}
        Error::BadRequest { .. } => {}
        Error::Server { .. } => {}
        Error::Unexpected { .. } => {}
        Error::MissingCallback { .. } => return Some(TelemetryEvent::InvalidUrlError),
        Error::DuplicatePlacementId { .. } => return None,
    }
    Some(TelemetryEvent::FetchError)
}

fn emit_telemetry_event(err: &Error) -> Result<()> {
    let mut url = Url::parse(DEFAULT_TELEMETRY_ENDPOINT)?;
    url.set_query(Some("event="));
    Request::get(url).send()?;
    Ok(())
}


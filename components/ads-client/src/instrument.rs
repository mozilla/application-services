/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use once_cell::sync::Lazy;
use serde_json::json;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;
use url::Url;
use viaduct::Request;

static TELEMETRY_ENDPOINT: Lazy<Url> = Lazy::new(|| {
    Url::parse("https://ads.mozilla.org/v1/log")
        .expect("hardcoded telemetry endpoint URL must be valid")
});

pub fn telemetry_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    TelemetryLayer {
        endpoint: TELEMETRY_ENDPOINT.clone(),
    }
    .with_filter(TelemetryFilter)
}

struct TelemetryLayer {
    endpoint: Url,
}

impl<S> Layer<S> for TelemetryLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let event_message = visitor
            .fields
            .get("message")
            .unwrap_or_default()
            .as_str()
            .unwrap_or_default();

        let mut url = self.endpoint.clone();
        url.set_query(Some(&format!("event={event_message}")));

        if let Err(e) = Request::get(url).send() {
            eprintln!("[TELEMETRY] Failed to send event: {}", e);
        }
    }
}

struct TelemetryFilter;

impl<S> tracing_subscriber::layer::Filter<S> for TelemetryFilter
where
    S: tracing::Subscriber,
{
    fn enabled(
        &self,
        meta: &tracing::Metadata<'_>,
        _cx: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        meta.target() == "ads_client::telemetry"
    }
}

#[derive(Default)]
struct EventVisitor {
    fields: serde_json::Map<String, serde_json::Value>,
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::String(format!("{:?}", value)),
        );
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields.insert(field.name().to_string(), json!(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(field.name().to_string(), json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(field.name().to_string(), json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.insert(field.name().to_string(), json!(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::mock;
    use tracing::error;
    use tracing_subscriber::prelude::*;

    #[test]
    fn test_telemetry_layer() {
        let subscriber = tracing_subscriber::registry::Registry::default().with(telemetry_layer());
        tracing::subscriber::with_default(subscriber, || {});
    }

    #[test]
    fn test_telemetry_sends_to_mock_server() {
        viaduct_dev::init_backend_dev();

        let mock_server_url = mockito::server_url();
        let telemetry_url = Url::parse(&format!("{}/v1/log", mock_server_url)).unwrap();

        let mock_endpoint = mock("GET", "/v1/log")
            .with_status(200)
            .match_query(mockito::Matcher::Regex(
                r#"event=test%20telemetry%20error"#.to_string(),
            ))
            .expect(1)
            .create();

        let telemetry_layer = TelemetryLayer {
            endpoint: telemetry_url,
        }
        .with_filter(TelemetryFilter);
        let subscriber = tracing_subscriber::registry::Registry::default().with(telemetry_layer);

        tracing::subscriber::with_default(subscriber, || {
            error!(target: "ads_client::telemetry", message = "test telemetry error");
            error!(target: "ads_client::not_telemetry", message = "non-telemetry event");
        });

        mock_endpoint.assert();
    }
}

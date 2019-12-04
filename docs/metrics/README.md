## Metrics collected by Application Services components

Some application-services components can collect telemetry using the [Glean SDK](https://mozilla.github.io/glean/).
This directory contains auto-generated documentation for all such metrics.

Collection of metrics is disabled by default, since it requires data review on a per-application basis.
Products that send telemetry via Glean and use the below components may opt-in to telemetry by:

* Reviewing the details of the metrics collected by each component (linked below) and determining
the current version number for each component's metrics.
* Obtaining a data-review to collect those metrics, following the [the Firefox Data Collection
process](https://wiki.mozilla.org/Firefox/Data_Collection).
* At application startup, calling the `enableTelemetry` function exposed by each component,
passing the version number obtained above.

### Components that collect telemetry

* [logins](./logins/metrics.md); currently on metrics version number `1`.

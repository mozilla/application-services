/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt;

use crate::{
    config::{Application, ReleaseChannel, TeamConfig, Unit},
    schema::{
        CustomVariable, Dashboard, DashboardBuilder, LogPanel, Panel, TeamDashboardsBuilder,
        TextPanel, TimeSeriesPanel,
    },
    sql::Query,
    util::{Join, UrlBuilder},
    Result,
};

/// Build the Sync dashboards
///
/// This adds sync panels to the main dashboard and also creates an extra dashboard for sync
/// details.
///
/// `sync_engines` are the engines your team owns
pub fn build_dashboards(
    config: &TeamConfig,
    builder: &mut TeamDashboardsBuilder,
    sync_engines: &[SyncEngine],
) -> Result<()> {
    add_overview_panels(
        &mut builder.main,
        "Sync success rate",
        config,
        sync_engines,
        SyncMetric::SuccessRate,
    );
    add_overview_panels(
        &mut builder.main,
        "Sync: total counts",
        config,
        sync_engines,
        SyncMetric::TotalCounts,
    );
    add_overview_panels(
        &mut builder.main,
        "Sync: average time",
        config,
        sync_engines,
        SyncMetric::AverageTime,
    );

    if config.team_name == "SYNC" {
        builder.main.add_panel_title("Legacy dashboards");
        builder.main.add_panel_full(sync_legacy_dashboard_panel());
    }
    builder.extra.push(extra_dashboard(config, sync_engines)?);

    Ok(())
}

fn extra_dashboard(config: &TeamConfig, sync_engines: &[SyncEngine]) -> Result<Dashboard> {
    let mut builder = DashboardBuilder::new(config, details_dashboard_slug(), "Sync Details");
    builder.add_application_variable();
    builder.add_channel_variable();
    builder.add_variable(CustomVariable::new("Sync Engine", "engine", sync_engines));

    builder.add_panel_title("Metrics");
    builder.add_panel_full(details_dash_count_panel(
        "Success Rate",
        SyncMetric::SuccessRate,
    ));
    builder.add_panel_full(details_dash_count_panel(
        "Total counts (7 day moving average)",
        SyncMetric::TotalCounts,
    ));
    builder.add_panel_full(details_dash_count_panel(
        "Average sync time (7 day moving average)",
        SyncMetric::AverageTime,
    ));
    builder.add_panel_title("Errors");
    builder.add_panel_full(details_dash_error_count_panel(sync_engines));
    builder.add_panel_full(details_dash_error_log_panel(sync_engines));

    Ok(builder.dashboard)
}

fn details_dashboard_slug() -> &'static str {
    // Every once in a while uploading the JSON fails for me (BDK).
    // To hack around this, I bump the number at the end there.
    "sync-details2"
}

#[derive(Clone, Copy)]
enum SyncMetric {
    SuccessRate,
    TotalCounts,
    AverageTime,
}

impl SyncMetric {
    fn column_name(&self) -> &'static str {
        match &self {
            SyncMetric::SuccessRate => "success_rate",
            SyncMetric::TotalCounts => "count_total",
            SyncMetric::AverageTime => "avg_sync_time",
        }
    }

    fn moving_average(&self) -> bool {
        matches!(self, SyncMetric::TotalCounts | SyncMetric::AverageTime)
    }

    fn update_panel(&self, panel: TimeSeriesPanel) -> TimeSeriesPanel {
        match self {
            SyncMetric::SuccessRate => panel
                .with_soft_limits(99, 100)
                .with_axis_label("success rate"),
            SyncMetric::TotalCounts => panel
                .with_log_distribution(10)
                .with_unit(Unit::SiShort)
                .with_axis_label("total counts"),
            SyncMetric::AverageTime => panel
                .with_unit(Unit::Seconds)
                .with_axis_label("average sync time"),
        }
    }

    fn column_expr(&self) -> String {
        let column_name = self.column_name();
        if !self.moving_average() {
            column_name.into()
        } else {
            format!(
                "AVG({column_name}) OVER (
                PARTITION BY engine_name
                ORDER BY submission_date
                ROWS BETWEEN 6 PRECEDING AND CURRENT ROW
              ) AS {column_name}"
            )
        }
    }
}

fn add_overview_panels(
    builder: &mut DashboardBuilder,
    title: &str,
    config: &TeamConfig,
    sync_engines: &[SyncEngine],
    metric: SyncMetric,
) {
    builder.add_panel_title(title);

    for app in Application::all() {
        builder.add_panel_third(overview_panel(
            config,
            sync_engines,
            app,
            ReleaseChannel::Nightly,
            metric,
        ));
        builder.add_panel_third(overview_panel(
            config,
            sync_engines,
            app,
            ReleaseChannel::Beta,
            metric,
        ));
        builder.add_panel_third(overview_panel(
            config,
            sync_engines,
            app,
            ReleaseChannel::Release,
            metric,
        ));
    }
}

fn overview_panel(
    config: &TeamConfig,
    sync_engines: &[SyncEngine],
    application: Application,
    channel: ReleaseChannel,
    metric: SyncMetric,
) -> Panel {
    if application == Application::Ios && channel == ReleaseChannel::Nightly {
        // iOs doesn't have a nightly
        return TextPanel::new("## N/A").into();
    }

    let column_name = metric.column_name();
    let query = Query {
        select: vec![
            "TIMESTAMP(submission_date) as time".into(),
            "engine_name".into(),
            metric.column_expr(),
        ],
        from: format!("({COMBINED_SUBQUERY})"),
        where_: vec![
            "$__timeFilter(TIMESTAMP(submission_date))".into(),
            format!("channel = '{channel}'"),
            format!(
                "engine_name in ({})",
                sync_engines.iter().map(|e| format!("'{e}'")).join(", ")
            ),
            match application {
                Application::Desktop => "application = 'desktop'",
                Application::Ios => "application = 'firefox-ios'",
                Application::Android => "application = 'firefox-android'",
            }
            .into(),
        ],
        order_by: Some("time".into()),
        ..Query::default()
    };

    let mut p = TimeSeriesPanel::new_grouped(
        application.display_name(channel),
        query.sql(),
        column_name,
        "engine_name",
    )
    .with_link(
        "Errors",
        UrlBuilder::new_dashboard(config.dashboard_slug(details_dashboard_slug()))
            .with_time_range_param()
            .with_param("var-application", application.slug())
            .with_param("var-channel", channel.to_string())
            .with_param("var-engine", "${__data.fields[\"engine_name\"]}")
            .build(),
    );

    for engine in sync_engines.iter() {
        p = p.with_label_color(engine.to_string(), engine.discriminant(), false);
    }
    let p = metric.update_panel(p);
    p.into()
}

fn details_dash_count_panel(title: &str, metric: SyncMetric) -> Panel {
    let query = Query {
        select: vec![
            "TIMESTAMP(submission_date) as time".into(),
            metric.column_expr(),
        ],
        from: format!("(\n{COMBINED_SUBQUERY}\n)"),
        where_: vec![
            "$__timeFilter(TIMESTAMP(submission_date))".into(),
            "channel = '${channel}'".into(),
            "application=CASE '${application}'
                WHEN 'firefox_desktop' THEN 'desktop'
                WHEN 'firefox_android' THEN 'firefox-android'
                WHEN 'firefox_ios' THEN 'firefox-ios'
                ELSE '${application}'
            END"
            .into(),
            "engine_name = '${engine}'".into(),
        ],
        ..Query::default()
    };

    let mut p = TimeSeriesPanel::new(title, query.sql());
    p = metric.update_panel(p);
    p.into()
}

fn details_dash_error_count_panel(sync_engines: &[SyncEngine]) -> Panel {
    let query = Query {
        select: vec![
            "error".into(),
            "$__timeGroup(submission_timestamp, $__interval) as time".into(),
            "COUNT(*) as count".into(),
        ],
        where_: vec![
            "application='${application}'".into(),
            "engine_name = '${engine}'".into(),
            "normalized_channel = '${channel}'".into(),
            "$__timeFilter(submission_timestamp)".into(),
        ],
        from: format!("(\n{}\n)", error_subquery(sync_engines)),
        group_by: Some("1, 2".into()),
        order_by: Some("count DESC".into()),
        ..Query::default()
    };

    TimeSeriesPanel::new_grouped("Error counts by type", query.sql(), "count", "error").into()
}

fn details_dash_error_log_panel(sync_engines: &[SyncEngine]) -> Panel {
    let query = Query {
        select: vec![
            "CONCAT(IFNULL(error, 'unknown'), ': ', IFNULL(details, 'unknown')) as message".into(),
            "submission_timestamp".into(),
        ],
        from: format!("(\n{}\n)", error_subquery(sync_engines)),
        where_: vec![
            "engine_name = '${engine}'".into(),
            "normalized_channel = '${channel}'".into(),
            "application='${application}'".into(),
            "$__timeFilter(submission_timestamp)".into(),
        ],
        order_by: Some("submission_timestamp DESC".into()),
        limit: Some(1000),
        ..Query::default()
    };

    LogPanel::new("Error list", query.sql()).into()
}

// Subquery that combines errors from both the legacy and glean sync tables
fn error_subquery(sync_engines: &[SyncEngine]) -> String {
    let mut queries = vec![];

    // Desktop
    queries.push(
        "\
SELECT
  'firefox_desktop' as application,
  STRING(engine.name) AS engine_name,
  normalized_channel,
  JSON_VALUE(engine.failureReason, '$.name') AS error,
  JSON_VALUE(engine.failureReason, '$.error') AS details,
  submission_timestamp
FROM
  firefox_desktop.sync
CROSS JOIN
  UNNEST(JSON_QUERY_ARRAY(metrics.object.syncs_syncs)) AS syncs
CROSS JOIN
  UNNEST(JSON_QUERY_ARRAY(syncs,'$.engines')) AS engine
WHERE
  metrics IS NOT NULL
  AND engine.failureReason IS NOT NULL
  AND client_info.os NOT IN ('iOS', 'Android')"
            .to_string(),
    );

    queries.extend(
        sync_engines
            .iter()
            .filter(|engine| engine.enabled_on_android())
            .map(|engine| {
                let table_prefix = engine.to_string().replace("-", "_");
                format!(
                    "\
SELECT
    'firefox_android' as application,
    '{engine}' as engine_name,
    normalized_channel,
    failure_reason.key as error,
    failure_reason.value as details,
    submission_timestamp
FROM mozdata.fenix.{table_prefix}_sync
CROSS JOIN UNNEST(metrics.labeled_string.{table_prefix}_sync_v2_failure_reason) as failure_reason"
                )
            }),
    );
    queries.extend(
        sync_engines
            .iter()
            .filter(|engine| engine.enabled_on_ios())
            .map(|engine| {
                let table_prefix = engine.to_string().replace("-", "_");
                format!(
                    "\
SELECT
    'firefox_ios' as application,
    '{engine}' as engine_name,
    normalized_channel,
    failure_reason.key as error,
    failure_reason.value as details,
    submission_timestamp
FROM mozdata.firefox_ios.{table_prefix}_sync
CROSS JOIN UNNEST(metrics.labeled_string.{table_prefix}_sync_v2_failure_reason) as failure_reason"
                )
            }),
    );

    queries.join("\nUNION ALL\n")
}

fn sync_legacy_dashboard_panel() -> Panel {
    let content = "\
# Legacy Sync dashboards
* [iOS Credit Cards Verification Usage](https://sql.telemetry.mozilla.org/dashboard/ios-credit-cards-verification-usage)
* [Mobile Logins Verification Usage](https://sql.telemetry.mozilla.org/dashboard/mobile-logins-verification-usage?p_channel=org_mozilla_ios_firefox)
* [iOS FxA Keychain Rollout Enrollment](https://sql.telemetry.mozilla.org/dashboard/ios-credit-cards-key-regeneration-metrics)
";
    TextPanel::new(content).into()
}

/// Subquery that combines the desktop and mobile ETL tables
const COMBINED_SUBQUERY: &str = "\
SELECT 
    submission_date,
    channel,
    application,
    engine_name,
    success_rate,
    avg_sync_time,
    count_total
FROM
    moz-fx-data-shared-prod.sync_derived.desktop_v1

UNION ALL

SELECT 
    submission_date,
    channel,
    application,
    engine_name,
    success_rate,
    0 as avg_sync_time, -- TODO: make this work on Mobile
    count_total
FROM
    moz-fx-data-shared-prod.sync_derived.mobile_v1
";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SyncEngine {
    Addresses,
    Bookmarks,
    CreditCards,
    History,
    Logins,
    Prefs,
    RustLogins,
    Tabs,
    WebextStorage,
}

impl SyncEngine {
    pub fn supported_applications(&self) -> &[Application] {
        match self {
            Self::Addresses => &[Application::Desktop, Application::Android, Application::Ios],
            Self::Bookmarks => &[Application::Desktop, Application::Android, Application::Ios],
            Self::CreditCards => &[Application::Desktop, Application::Android, Application::Ios],
            Self::History => &[Application::Desktop, Application::Android, Application::Ios],
            Self::Logins => &[Application::Android, Application::Ios],
            Self::RustLogins => &[Application::Desktop],
            Self::Tabs => &[Application::Desktop, Application::Android, Application::Ios],
            Self::Prefs => &[Application::Desktop],
            Self::WebextStorage => &[Application::Desktop],
        }
    }

    pub fn enabled_on_android(&self) -> bool {
        self.supported_applications()
            .contains(&Application::Android)
    }

    pub fn enabled_on_ios(&self) -> bool {
        self.supported_applications().contains(&Application::Ios)
    }

    /// Enum variant discriminant
    ///
    /// This is used to generate color indexes, which keeps colors consistent across panels
    /// (SYNC-5459)".
    pub fn discriminant(&self) -> usize {
        match self {
            Self::Addresses => 0,
            Self::Bookmarks => 1,
            Self::CreditCards => 2,
            Self::History => 3,
            Self::Logins => 4,
            Self::RustLogins => 5,
            Self::Tabs => 6,
            Self::Prefs => 7,
            Self::WebextStorage => 8,
        }
    }
}

impl fmt::Display for SyncEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Addresses => write!(f, "addresses"),
            Self::Bookmarks => write!(f, "bookmarks"),
            Self::CreditCards => write!(f, "creditcards"),
            Self::History => write!(f, "history"),
            Self::Logins => write!(f, "logins"),
            Self::Prefs => write!(f, "prefs"),
            Self::RustLogins => write!(f, "rust-logins"),
            Self::Tabs => write!(f, "tabs"),
            Self::WebextStorage => write!(f, "webext-storage"),
        }
    }
}

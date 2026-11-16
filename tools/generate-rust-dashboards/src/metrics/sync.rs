/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt;

use crate::{
    config::{Application, ReleaseChannel, TeamConfig, Unit},
    schema::{
        CustomVariable, Dashboard, DashboardBuilder, DataLink, Datasource, FieldConfig,
        FieldConfigCustom, FieldConfigDefaults, FieldConfigOverride, FieldConfigOverrideMatcher,
        FieldConfigOverrideProperty, GridPos, LogOptions, LogPanel, Panel, ScaleDistribution,
        Target, TeamDashboardsBuilder, TextPanel, TimeSeriesPanel, Transformation,
    },
    sql::Query,
    util::{dashboard_count_color, Join, UrlBuilder},
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
    builder.add_variable(CustomVariable {
        label: "Sync Engine".into(),
        name: "engine".into(),
        query: sync_engines.iter().map(|e| e.to_string()).join(","),
        ..CustomVariable::default()
    });

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

    fn field_config_custom(&self) -> FieldConfigCustom {
        match self {
            SyncMetric::SuccessRate => FieldConfigCustom {
                axis_label: "success rate".into(),
                axis_soft_min: 99,
                axis_soft_max: 100,
                ..FieldConfigCustom::default()
            },
            SyncMetric::TotalCounts => FieldConfigCustom {
                scale_distribution: ScaleDistribution {
                    type_: "log".into(),
                    log: Some(10),
                },
                ..FieldConfigCustom::default()
            },
            SyncMetric::AverageTime => FieldConfigCustom::default(),
        }
    }

    fn unit(&self) -> Option<Unit> {
        match self {
            SyncMetric::SuccessRate => None,
            SyncMetric::TotalCounts => Some(Unit::SiShort),
            SyncMetric::AverageTime => Some(Unit::Seconds),
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
        return TextPanel {
            content: "## N/A".into(),
            mode: "markdown".into(),
            grid_pos: GridPos::height(8),
        }
        .into();
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

    let mut color_overrides = vec![];
    for engine in sync_engines.iter() {
        // Use the SyncEngine discriminant to keep colors consistent across all panels (SYNC-5459)
        color_overrides.push(FieldConfigOverride {
            matcher: FieldConfigOverrideMatcher {
                id: "byName".into(),
                options: engine.to_string(),
            },
            properties: vec![engine.dashboard_color()],
        });
    }

    Panel::from(TimeSeriesPanel {
        title: application.display_name(channel),
        grid_pos: GridPos::height(8),
        datasource: Datasource::bigquery(),
        // needs to be fairly large since the total sync count can be low on mobile/nightly
        interval: "1d".into(),
        targets: vec![Target::table(query.sql())],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                links: vec![DataLink {
                    url: UrlBuilder::new_dashboard(config.dashboard_slug(details_dashboard_slug()))
                        .with_time_range_param()
                        .with_param("var-application", application.slug())
                        .with_param("var-channel", channel.to_string())
                        .with_param("var-engine", "${__data.fields[\"engine_name\"]}")
                        .build(),
                    target_blank: true,
                    one_click: true,
                    title: "Errors".into(),
                }],
                custom: metric.field_config_custom(),
                unit: metric.unit(),
            },
            overrides: color_overrides,
        },
        transformations: vec![
            Transformation::PartitionByValues {
                fields: vec!["engine_name".into()],
                keep_fields: true,
            },
            // Fixup the field names for better legend labels
            Transformation::RenameByRegex {
                regex: format!("{column_name} (.*)"),
                rename_pattern: "$1".into(),
            },
        ],
        ..TimeSeriesPanel::default()
    })
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

    TimeSeriesPanel {
        title: title.into(),
        grid_pos: GridPos::height(10),
        datasource: Datasource::bigquery(),
        // needs to be fairly large since the total sync count can be low on mobile/nightly
        interval: "1d".into(),
        targets: vec![Target::table(query.sql())],
        transformations: vec![],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                links: vec![],
                custom: metric.field_config_custom(),
                unit: metric.unit(),
            },
            ..FieldConfig::default()
        },
        ..TimeSeriesPanel::default()
    }
    .into()
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

    TimeSeriesPanel {
        title: "Error counts by type".into(),
        grid_pos: GridPos::height(10),
        datasource: Datasource::bigquery(),
        // needs to be fairly large since the total sync count can be low on mobile/nightly
        interval: "1d".into(),
        targets: vec![Target::table(query.sql())],
        transformations: vec![
            Transformation::PartitionByValues {
                fields: vec!["error".into()],
                keep_fields: true,
            },
            Transformation::RenameByRegex {
                regex: "count (.*)".into(),
                rename_pattern: "$1".into(),
            },
        ],
        ..TimeSeriesPanel::default()
    }
    .into()
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

    LogPanel {
        title: "Error list".into(),
        grid_pos: GridPos::height(20),
        datasource: Datasource::bigquery(),
        targets: vec![Target::table(query.sql())],
        options: LogOptions {
            enable_log_details: false,
            ..LogOptions::default()
        },
        ..LogPanel::default()
    }
    .into()
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
    TextPanel {
        content: content.to_string(),
        mode: "markdown".into(),
        grid_pos: GridPos::height(8),
    }
    .into()
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
    /// Dashboard color for this engine.
    ///
    /// This method helps keep colors consistent across all panels (SYNC-5459)
    pub fn dashboard_color(&self) -> FieldConfigOverrideProperty {
        match self {
            Self::Addresses => dashboard_count_color(0, false),
            Self::Bookmarks => dashboard_count_color(1, false),
            Self::CreditCards => dashboard_count_color(2, false),
            Self::History => dashboard_count_color(3, false),
            Self::Logins => dashboard_count_color(4, false),
            Self::RustLogins => dashboard_count_color(5, false),
            Self::Tabs => dashboard_count_color(6, false),
            Self::Prefs => dashboard_count_color(7, false),
            Self::WebextStorage => dashboard_count_color(8, false),
        }
    }

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

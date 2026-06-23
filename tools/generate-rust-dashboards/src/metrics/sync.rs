/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, ReleaseChannel, TeamConfig},
    schema::{
        CalculateFieldOptions, CustomVariable, Dashboard, DashboardBuilder, DataLink, Datasource,
        FieldConfig, FieldConfigCustom, FieldConfigDefaults, GridPos, LogOptions, LogPanel, Panel,
        Target, TextPanel, TimeSeriesPanel, Transformation, WindowFunctionWindow,
    },
    sql::Query,
    util::{Join, UrlBuilder},
    Result,
};

pub fn add_to_main_dashboard(builder: &mut DashboardBuilder, config: &TeamConfig) -> Result<()> {
    builder.add_panel_title("Sync");

    for app in config.applications().iter() {
        builder.add_panel_third(overview_count_panel(config, *app, ReleaseChannel::Nightly));
        builder.add_panel_third(overview_count_panel(config, *app, ReleaseChannel::Beta));
        builder.add_panel_third(overview_count_panel(config, *app, ReleaseChannel::Release));
    }
    Ok(())
}

pub fn extra_dashboard(config: &TeamConfig) -> Result<Dashboard> {
    let mut builder = DashboardBuilder::new(
        format!("{} - Sync Details", config.team_name),
        format!("{}-sync-details", config.team_slug()),
    );
    builder.add_application_variable(config)?;
    builder.add_channel_variable();
    builder.add_variable(CustomVariable {
        label: "Sync Engine".into(),
        name: "engine".into(),
        query: config
            .components
            .iter()
            .flat_map(|a| a.sync_engines())
            .map(|s| s.to_string())
            .join(","),
        ..CustomVariable::default()
    });

    builder.add_panel_title("Metrics");
    builder.add_panel_full(details_dash_count_panel(
        "Success Rate",
        "success_rate",
        false,
    ));
    builder.add_panel_full(details_dash_count_panel(
        "Total counts (7 day moving average)",
        "count_total",
        true,
    ));
    builder.add_panel_title("Errors");
    builder.add_panel_full(details_dash_error_count_panel(config));
    builder.add_panel_full(details_dash_error_log_panel(config));

    Ok(builder.dashboard)
}

fn overview_count_panel(
    config: &TeamConfig,
    application: Application,
    channel: ReleaseChannel,
) -> Panel {
    if application == Application::Ios && channel == ReleaseChannel::Nightly {
        return TextPanel {
            content: "## N/A".into(),
            mode: "markdown".into(),
            grid_pos: GridPos::height(8),
        }
        .into();
    }

    let query = count_query(config, application, format!("'{channel}'"));

    Panel::from(TimeSeriesPanel {
        title: application.display_name(channel),
        grid_pos: GridPos::height(8),
        datasource: Datasource::bigquery(),
        // needs to be fairly large since the total sync count can be low on mobile/nightly
        interval: "1d".into(),
        targets: vec![Target::table(query)],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                links: vec![DataLink {
                    url: UrlBuilder::new_dashboard(format!("{}-sync-details", config.team_slug()))
                        .with_time_range_param()
                        .with_param("var-application", application.slug())
                        .with_param("var-channel", channel.to_string())
                        .with_param("var-engine", "${__data.fields[\"engine_name\"]}")
                        .build(),
                    target_blank: true,
                    one_click: true,
                    title: "Errors".into(),
                }],
                custom: FieldConfigCustom {
                    axis_label: "success rate".into(),
                    axis_soft_min: 99,
                    axis_soft_max: 100,
                    ..FieldConfigCustom::default()
                },
                unit: None,
            },
        },
        transformations: vec![
            Transformation::PartitionByValues {
                fields: vec!["engine_name".into()],
                keep_fields: true,
            },
            // Fixup the field names for better legend labels
            Transformation::RenameByRegex {
                regex: "success_rate (.*)".into(),
                rename_pattern: "$1".into(),
            },
        ],
        ..TimeSeriesPanel::default()
    })
}

/// Query to fetch sync success rates
fn count_query(config: &TeamConfig, application: Application, channel_expr: String) -> String {
    let table_name = if application == Application::Desktop {
        "desktop_v1"
    } else {
        "mobile_v1"
    };
    let application_where = match application {
        Application::Desktop => "application = 'desktop'",
        Application::Ios => "application = 'firefox-ios'",
        Application::Android => "application = 'firefox-android'",
    };

    let mut engines: Vec<_> = config
        .components
        .iter()
        .flat_map(|c| c.sync_engines())
        .map(|e| format!("'{e}'"))
        .collect();
    engines.sort_unstable();
    engines.dedup();
    let engines_where = format!("engine_name IN ({})", engines.join(", "));

    format!(
        "\
SELECT 
    TIMESTAMP(submission_date) as time,
    engine_name,
    success_rate
FROM
    moz-fx-data-shared-prod.sync_derived.{table_name}
WHERE
    channel = {channel_expr}
    AND $__timeFilter(TIMESTAMP(submission_date))
    AND {application_where}
    AND {engines_where}
ORDER BY time"
    )
}

fn details_dash_count_panel(title: &str, column_name: &str, moving_average: bool) -> Panel {
    let query = Query {
        select: vec!["time".into(), column_name.into()],
        from: format!("(\n{}\n)", details_dash_count_query()),
        group_by: Some("1, 2".into()),
        ..Query::default()
    };

    let transformations = if moving_average {
        vec![Transformation::CalculateField(
            CalculateFieldOptions::WindowFunctions {
                replace_fields: true,
                window: WindowFunctionWindow {
                    field: "count_total".into(),
                    reducer: "mean".into(),
                    window_alignment: "centered".into(),
                    window_size: 7.0,
                    window_size_mode: "fixed".into(),
                },
            },
        )]
    } else {
        vec![]
    };

    TimeSeriesPanel {
        title: title.into(),
        grid_pos: GridPos::height(10),
        datasource: Datasource::bigquery(),
        // needs to be fairly large since the total sync count can be low on mobile/nightly
        interval: "1d".into(),
        targets: vec![Target::table(query.sql())],
        transformations,
        ..TimeSeriesPanel::default()
    }
    .into()
}

/// Query to count metrics for the details dashboard
fn details_dash_count_query() -> String {
    "\
SELECT 
    TIMESTAMP(submission_date) as time,
    success_rate,
    count_total
FROM
    moz-fx-data-shared-prod.sync_derived.desktop_v1
WHERE
    $__timeFilter(TIMESTAMP(submission_date))
    AND channel = '${channel}'
    AND application=CASE '${application}'
        WHEN 'firefox_desktop' THEN 'desktop'
        WHEN 'firefox_android' THEN 'firefox-android'
        WHEN 'firefox_ios' THEN 'firefox-ios'
        ELSE '${application}'
    END
    AND engine_name = '${engine}'

UNION ALL

SELECT 
    TIMESTAMP(submission_date) as time,
    success_rate,
    count_total
FROM
    moz-fx-data-shared-prod.sync_derived.mobile_v1
WHERE
    $__timeFilter(TIMESTAMP(submission_date))
    AND channel = '${channel}'
    AND application=CASE '${application}'
        WHEN 'firefox_desktop' THEN 'desktop'
        WHEN 'firefox_android' THEN 'firefox-android'
        WHEN 'firefox_ios' THEN 'firefox-ios'
        ELSE '${application}'
    END
    AND engine_name = '${engine}'
ORDER BY time"
        .to_string()
}

fn details_dash_error_count_panel(config: &TeamConfig) -> Panel {
    let query = Query {
        select: vec![
            "error".into(),
            "$__timeGroup(submission_timestamp, $__interval) as time".into(),
            "COUNT(*) as count".into(),
        ],
        where_: vec![
            "application='${application}'".into(),
            "engine_name='${engine}'".into(),
            "normalized_channel = '${channel}'".into(),
            "$__timeFilter(submission_timestamp)".into(),
        ],
        from: format!("(\n{}\n)", error_subquery(config)),
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

fn details_dash_error_log_panel(config: &TeamConfig) -> Panel {
    let query = Query {
        select: vec![
            "CONCAT(IFNULL(error, 'unknown'), ': ', IFNULL(details, 'unknown')) as message".into(),
            "submission_timestamp".into(),
        ],
        from: format!("(\n{}\n)", error_subquery(config)),
        where_: vec![
            "engine_name='${engine}'".into(),
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
fn error_subquery(config: &TeamConfig) -> String {
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
        config
            .components
            .iter()
            .flat_map(|c| c.sync_engines())
            .flat_map(|engine_name| {
                [
                    format!(
                        "\
    SELECT
        'firefox_android' as application,
        '{engine_name}' as engine_name,
        normalized_channel,
        failure_reason.key as error,
        failure_reason.value as details,
        submission_timestamp
    FROM mozdata.fenix.{engine_name}_sync
    CROSS JOIN UNNEST(metrics.labeled_string.{engine_name}_sync_v2_failure_reason) as failure_reason"
                    ),
                    format!(
                        "\
    SELECT
        'firefox_ios' as application,
        '{engine_name}' as engine_name,
        normalized_channel,
        failure_reason.key as error,
        failure_reason.value as details,
        submission_timestamp
    FROM mozdata.firefox_ios.{engine_name}_sync
    CROSS JOIN UNNEST(metrics.labeled_string.{engine_name}_sync_v2_failure_reason) as failure_reason"
                    ),
                ]
            }),
    );

    queries.join("\nUNION ALL\n")
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, ReleaseChannel, TeamConfig},
    schema::{
        CustomVariable, Dashboard, DashboardBuilder, DataLink, Datasource, FieldConfig,
        FieldConfigCustom, FieldConfigDefaults, GridPos, LogPanel, Panel, PieChartPanel, Target,
        TimeSeriesPanel, Transformation,
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
        format!("{} - Sync Errors", config.team_name),
        format!("{}-sync-errors", config.team_slug()),
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
    builder.add_filter_sql_variable();

    builder.add_panel_full(error_list_count_panel(config));
    builder.add_panel_full(error_list_log_panel(config));

    Ok(builder.dashboard)
}

fn overview_count_panel(
    config: &TeamConfig,
    application: Application,
    channel: ReleaseChannel,
) -> Panel {
    let query = if application == Application::Desktop {
        desktop_count_query(format!("'{channel}'"))
    } else {
        mobile_count_query(config, format!("'{channel}'"))
    };

    TimeSeriesPanel {
        title: application.display_name(channel),
        grid_pos: GridPos::height(8),
        datasource: Datasource::bigquery(),
        // needs to be fairly large since the total sync count can be low on mobile/nightly
        interval: "1d".into(),
        targets: vec![Target::table(query)],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                links: vec![DataLink {
                    url: UrlBuilder::new_dashboard(format!("{}-sync-errors", config.team_slug()))
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
                    axis_soft_min: 90,
                    axis_soft_max: 100,
                    ..FieldConfigCustom::default()
                },
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
    }
    .into()
}

/// Subquery to fetch general sync info for desktop
///
/// We use subqueries to smooth out the differences between desktop and mobile telemetry.
fn desktop_count_query(channel_expr: String) -> String {
    // TODO: switch from `telemetry.sync` to the newer glean ping
    format!(
        "\
SELECT
    engine.name as engine_name,
    $__timeGroup(submission_timestamp, $__interval) as time,
    SAFE_DIVIDE(
        -- 100 * success count
        100 * COUNTIF(
            syncs.failure_reason IS NULL
            AND engine.failure_reason IS NULL
            AND (engine.incoming IS NOT NULL OR ARRAY_LENGTH(engine.outgoing) > 0)
        ),
        -- count success or failures
        COUNTIF(
            syncs.failure_reason IS NOT NULL
            OR engine.failure_reason IS NOT NULL
            OR (engine.incoming IS NOT NULL OR ARRAY_LENGTH(engine.outgoing) > 0)
        )
    ) AS success_rate,
FROM telemetry.sync
CROSS JOIN UNNEST(payload.syncs) as syncs
CROSS JOIN UNNEST(syncs.engines) as engine
WHERE normalized_channel = {channel_expr}
    AND engine.name <> 'extension-storage'
    AND engine.name <> 'bookmarks'
    AND $__timeFilter(submission_timestamp)
GROUP BY 1, 2
ORDER BY engine_name, time"
    )
}

/// Subquery to fetch general sync info for mobile
///
/// We use subqueries to smooth out the differences between desktop and mobile telemetry.
fn mobile_count_query(config: &TeamConfig, channel_expr: String) -> String {
    let parts = config
        .components
        .iter()
        .flat_map(|c| c.sync_engines())
        .map(|engine_name| {
            let table_name = format!("{}_sync", engine_name.replace("-", "_"));
            format!(
                "\
SELECT '{engine_name}' AS engine_name, 
    $__timeGroup(submission_timestamp, $__interval) as time,
    SAFE_DIVIDE(
        -- 100 * success count
        100 * COUNTIF(
            (metrics.labeled_counter.{table_name}_v2_incoming IS NOT NULL
              OR metrics.labeled_counter.{table_name}_v2_outgoing IS NOT NULL)
            AND metrics.labeled_string.{table_name}_v2_failure_reason IS NULL
        ),
        -- count success or failures
        COUNTIF(
            metrics.labeled_string.{table_name}_v2_failure_reason IS NOT NULL
            OR metrics.labeled_counter.{table_name}_v2_outgoing IS NOT NULL
            OR metrics.labeled_counter.{table_name}_v2_incoming IS NOT NULL
        )
    ) AS success_rate,
FROM mozdata.fenix.{table_name}
WHERE normalized_channel={channel_expr} AND $__timeFilter(submission_timestamp)
GROUP BY 1, 2"
            )
        })
        .collect::<Vec<_>>();
    format!(
        "{}\nORDER BY engine_name, time",
        parts.join("\nUNION ALL\n")
    )
}

fn error_list_count_panel(config: &TeamConfig) -> Panel {
    let query = Query {
        select: vec!["message".into(), "COUNT(*) as count".into()],
        where_: vec![
            "engine_name='${engine}'".into(),
            "normalized_channel = '${channel}'".into(),
            "$__timeFilter(time)".into(),
        ],
        from: format!("(\n{}\n)", error_subquery(config)),
        group_by: Some("1".into()),
        order_by: Some("count DESC".into()),
        ..Query::default()
    };

    PieChartPanel {
        title: "".into(),
        grid_pos: GridPos::height(10),
        datasource: Datasource::bigquery(),
        interval: "30m".into(),
        targets: vec![Target::timeseries(query.sql())],
        ..PieChartPanel::default()
    }
    .into()
}

fn error_list_log_panel(config: &TeamConfig) -> Panel {
    let query = Query {
        select: vec!["message".into(), "time".into()],
        from: format!("(\n{}\n)", error_subquery(config)),
        where_: vec![
            "engine_name='${engine}'".into(),
            "normalized_channel = '${channel}'".into(),
            "$__timeFilter(time)".into(),
        ],
        order_by: Some("time DESC".into()),
        limit: Some(1000),
        ..Query::default()
    };

    LogPanel {
        title: "Error list".into(),
        grid_pos: GridPos::height(20),
        datasource: Datasource::bigquery(),
        targets: vec![Target::table(query.sql())],
        ..LogPanel::default()
    }
    .into()
}

// Subquery that combines errors from both the legacy and glean sync tables
fn error_subquery(config: &TeamConfig) -> String {
    let mut queries = vec![];

    // Desktop needs
    queries.push("\
SELECT CONCAT(IFNULL(engine.failure_reason.name, 'unknown'), ': ', IFNULL(engine.failure_reason.error, '')) as message,
        submission_timestamp as time,
        engine.name as engine_name,
        normalized_channel
FROM telemetry.sync
CROSS JOIN UNNEST(payload.syncs) as syncs
CROSS JOIN UNNEST(syncs.engines) as engine
WHERE engine.failure_reason IS NOT NULL
".to_string());

    queries.extend(
        config
            .components
            .iter()
            .flat_map(|c| c.sync_engines())
            .map(|engine_name| {
                format!(
                    "\
SELECT CONCAT(failure_reason.key, ': ', failure_reason.value) as message,
    submission_timestamp as time,
    '{engine_name}' as engine_name,
    normalized_channel
FROM mozdata.fenix.{engine_name}_sync
CROSS JOIN UNNEST(metrics.labeled_string.{engine_name}_sync_v2_failure_reason) as failure_reason"
                )
            }),
    );

    queries.join("\nUNION ALL\n")
}

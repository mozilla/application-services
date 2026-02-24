/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Handles the `rust_component_errors` ping.

use crate::{
    config::{Application, ReleaseChannel, TeamConfig},
    schema::{
        Dashboard, DashboardBuilder, DataLink, Datasource, FieldConfig, FieldConfigCustom,
        FieldConfigDefaults, GridPos, LogPanel, Panel, QueryVariable, QueryVariableQuery, Target,
        TextBoxVariable, TimeSeriesPanel, Transformation, VariableSortOrder,
    },
    sql::Query,
    util::UrlBuilder,
    Result,
};

pub fn add_to_dashboard(builder: &mut DashboardBuilder, config: &TeamConfig) -> Result<()> {
    builder.add_panel_title("Component Errors");

    for app in config.applications().iter() {
        builder.add_panel_third(count_panel(config, *app, ReleaseChannel::Nightly));
        builder.add_panel_third(count_panel(config, *app, ReleaseChannel::Beta));
        builder.add_panel_third(count_panel(config, *app, ReleaseChannel::Release));
    }

    Ok(())
}

fn count_panel(config: &TeamConfig, application: Application, channel: ReleaseChannel) -> Panel {
    let mut query = Query {
        prep_statements: error_type_re_prep_statements(config),
        select: vec![
            "$__timeGroup(submission_timestamp, $__interval) as time".into(),
            "metrics.string.rust_component_errors_error_type as error_type".into(),
        ],
        from: format!(
            "mozdata.{}.rust_component_errors",
            application.bigquery_dataset()
        ),
        where_: vec![
            format!("normalized_channel = '{channel}'"),
            "$__timeFilter(submission_timestamp)".into(),
            "metrics.string.rust_component_errors_error_type IS NOT NULL".into(),
            "REGEXP_CONTAINS(metrics.string.rust_component_errors_error_type, error_type_re)"
                .into(),
        ],
        group_by: Some("1, 2".into()),
        order_by: Some("error_type, time".into()),
        ..Query::default()
    };
    query.add_count_per_day_column("COUNT(*)", "errors");

    TimeSeriesPanel {
        title: application.display_name(channel),
        grid_pos: GridPos::height(8),
        datasource: Datasource::bigquery(),
        interval: "1h".into(),
        targets: vec![Target::table(query.sql())],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                links: vec![DataLink {
                    url: UrlBuilder::new_dashboard(format!("{}-errors", config.team_slug()))
                        .with_time_range_param()
                        .with_param("var-application", application.slug())
                        .with_param("var-channel", channel.to_string())
                        .with_param("var-error_type", "${__data.fields[\"error_type\"]}")
                        .build(),
                    target_blank: true,
                    one_click: true,
                    title: "Error list".into(),
                }],
                custom: FieldConfigCustom {
                    axis_label: "errors / day".into(),
                    ..FieldConfigCustom::default()
                },
            },
        },
        transformations: vec![
            Transformation::PartitionByValues {
                fields: vec!["error_type".into()],
                keep_fields: true,
            },
            // Fixup the field names for better legend labels
            Transformation::RenameByRegex {
                regex: "errors (.*)".into(),
                rename_pattern: "$1".into(),
            },
        ],
        ..TimeSeriesPanel::default()
    }
    .into()
}

pub fn extra_dashboard(config: &TeamConfig) -> Result<Dashboard> {
    let mut builder = DashboardBuilder::new(
        format!("{} - Error List", config.team_name),
        format!("{}-errors", config.team_slug()),
    );
    builder.add_application_variable(config)?;
    builder.add_channel_variable();
    builder.add_variable(error_type_variable());
    builder.add_variable(version_variable());
    builder.add_variable(build_date_variable());
    builder.add_variable(TextBoxVariable {
        label: "Search details".into(),
        name: "details".into(),
        ..TextBoxVariable::default()
    });
    builder.add_filter_sql_variable();

    builder.add_panel_full(error_list_count_panel());
    builder.add_panel_full(error_list_log_panel());

    Ok(builder.dashboard)
}

pub fn error_type_variable() -> QueryVariable {
    let query = QueryVariableQuery::from_sql(
        "\
SELECT DISTINCT metrics.string.rust_component_errors_error_type
FROM mozdata.fenix.rust_component_errors
WHERE submission_timestamp > TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL 14 day)
    AND metrics.string.rust_component_errors_error_type IS NOT NULL
    AND metrics.string.rust_component_errors_error_type <> ''
ORDER BY metrics.string.rust_component_errors_error_type",
    );

    QueryVariable {
        label: "Error type".into(),
        name: "error_type".into(),
        datasource: Datasource::bigquery(),
        query,
        ..QueryVariable::default()
    }
}

pub fn version_variable() -> QueryVariable {
    let query = QueryVariableQuery::from_sql(
        "\
SELECT 'All' as text, '' as value
UNION ALL
SELECT version as text, version as value
FROM (
    SELECT DISTINCT CAST(mozfun.norm.extract_version(client_info.app_display_version, 'major') AS STRING) as version
    FROM mozdata.fenix.rust_component_errors
    WHERE submission_timestamp > TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL 14 day)
        AND mozfun.norm.extract_version(client_info.app_display_version, 'major') IS NOT NULL
    ORDER BY 1 DESC
)",
    );

    QueryVariable {
        label: "Version".into(),
        name: "version".into(),
        datasource: Datasource::bigquery(),
        query,
        sort: Some(VariableSortOrder::AlphabeticalDescending),
        ..QueryVariable::default()
    }
}

pub fn build_date_variable() -> QueryVariable {
    let query = QueryVariableQuery::from_sql(
        "\
SELECT 'None' as text, '' as value
UNION ALL
SELECT build_date as text, build_date as value
FROM (
    SELECT DISTINCT SUBSTR(client_info.build_date, 0, 10) as build_date
    FROM mozdata.fenix.rust_component_errors
    WHERE submission_timestamp > TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL 14 day)
    ORDER BY 1 DESC
)",
    );

    QueryVariable {
        label: "Group by build date".into(),
        name: "build_date".into(),
        datasource: Datasource::bigquery(),
        sort: Some(VariableSortOrder::AlphabeticalDescending),
        query,
        ..QueryVariable::default()
    }
}

fn error_list_count_panel() -> Panel {
    let mut query = Query {
        select: vec![
            "$__timeGroup(submission_timestamp, $__interval) as time".into(),
            "IF('${build_date}' = '', '', IF(build_date < '${build_date}', '< ${build_date}', '>= ${build_date}')) as build_date".into(),
        ],
        where_: vec![
            "error_type='${error_type}'".into(),
            "$__timeFilter(submission_timestamp)".into(),
            "normalized_channel = '${channel}'".into(),
            "'${version}' = '' OR version = CAST('${version}' AS NUMERIC)".into(),
            "'${details}' = '' OR details LIKE '%${details}%'".into(),
            "${filter_sql}".into(),
        ],
        from: error_subquery().as_subquery(),
        group_by: Some("1, 2".into()),
        order_by: Some("1 ASC, 2 ASC".into()),
        ..Query::default()
    };
    query.add_count_per_day_column("COUNT(*)", "errors");

    TimeSeriesPanel {
        title: "".into(),
        grid_pos: GridPos::height(10),
        datasource: Datasource::bigquery(),
        interval: "30m".into(),
        targets: vec![Target::timeseries(query.sql())],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                custom: FieldConfigCustom {
                    axis_label: "errors / day".into(),
                    ..FieldConfigCustom::default()
                },
                ..FieldConfigDefaults::default()
            },
        },
        ..TimeSeriesPanel::default()
    }
    .into()
}

fn error_list_log_panel() -> Panel {
    let mut query = Query {
        select: vec![
            "CONCAT(error_type, ': ', details) as message".into(),
            "error_type".into(),
            "details".into(),
            "ARRAY_TO_STRING(breadcrumbs, '\\n') as breadcrumbs".into(),
        ],
        where_: vec![
            "error_type='${error_type}'".into(),
            "$__timeFilter(submission_timestamp)".into(),
            "normalized_channel = '${channel}'".into(),
            "'${version}' = '' OR version = CAST('${version}' AS NUMERIC)".into(),
            "('${details}' = '' OR details LIKE '%${details}%')".into(),
            "${filter_sql}".into(),
        ],
        from: error_subquery().as_subquery(),
        order_by: Some("submission_timestamp DESC".into()),
        limit: Some(1000),
        ..Query::default()
    };
    query.add_standard_glean_columns_no_prefix();

    LogPanel {
        title: "Error list".into(),
        grid_pos: GridPos::height(20),
        datasource: Datasource::bigquery(),
        targets: vec![Target::table(query.sql())],
        ..LogPanel::default()
    }
    .into()
}

// Select everything from `rust_component_errors_error_type`, but "flatten" the column names.
//
// This means `error_type` instead of `metrics.string.rust_component_errors_error_type`, which is
// needed to make the filters work.
fn error_subquery() -> Query {
    let mut subquery = Query {
        select: vec![
            "SUBSTR(client_info.build_date, 0, 10) as build_date".into(),
            "mozfun.norm.extract_version(client_info.app_display_version, 'major') as version"
                .into(),
            "metrics.string.rust_component_errors_error_type as error_type".into(),
            "metrics.string.rust_component_errors_details as details".into(),
            "metrics.string_list.rust_component_errors_breadcrumbs as breadcrumbs".into(),
            "normalized_channel".into(),
        ],
        ..Query::default()
    };
    subquery.add_standard_glean_columns();
    subquery.add_from_using_application_var("rust_component_errors");
    subquery
}

/// Bigquery statements to define the `error_type_re` variable
///
/// This is a bigquery variable created from the `components` grafana variable.
/// We use it as a regex to match error pings against.
fn error_type_re_prep_statements(config: &TeamConfig) -> Vec<String> {
    // `error_type_re` variable;
    let mut query_parts = vec![];
    query_parts.push("SELECT CASE value".into());
    for c in config.components.iter() {
        query_parts.push(format!("WHEN '{}' THEN '^{}'", c.slug(), c.error_prefix()));
    }
    query_parts.push("END".into());
    query_parts.push("FROM UNNEST(SPLIT('${components:csv}', ',')) as value".into());
    vec![
        "DECLARE error_type_re STRING".into(),
        format!(
            "SET error_type_re = ARRAY_TO_STRING(ARRAY({}), '|')",
            query_parts.join(" ")
        ),
    ]
}

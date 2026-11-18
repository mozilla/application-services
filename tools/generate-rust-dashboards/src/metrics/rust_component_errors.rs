/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Handles the `rust_component_errors` ping.

use crate::{
    config::{Application, ReleaseChannel, TeamConfig},
    schema::{
        CustomVariable, Dashboard, DashboardBuilder, LogPanel, Panel, QueryVariable,
        TeamDashboardsBuilder, TextBoxVariable, TextPanel, TimeSeriesPanel, VariableSortOrder,
    },
    sql::{Query, Union},
    util::UrlBuilder,
    Result,
};

/// Build the Component Error dashboards
///
/// This adds error panels to the main dashboard and also creates an extra dashboard for error
/// details.
///
/// `components` are the components your team owns
/// with each value corresponding to an error type prefix.
/// Component `foo` will match error types that start with `foo-`.
pub fn build_dashboards(
    config: &TeamConfig,
    builder: &mut TeamDashboardsBuilder,
    components: &[&str],
) -> Result<()> {
    builder.main.add_panel_title("Component Errors");

    for app in Application::all() {
        builder.main.add_panel_third(count_panel(
            config,
            components,
            app,
            ReleaseChannel::Nightly,
        ));
        builder
            .main
            .add_panel_third(count_panel(config, components, app, ReleaseChannel::Beta));
        builder.main.add_panel_third(count_panel(
            config,
            components,
            app,
            ReleaseChannel::Release,
        ));
    }

    builder.main.add_variable(
        CustomVariable::new(
            "Component Errors Filter",
            "rust_component_errors_filter",
            components,
        )
        .with_multi_select()
        .with_all_selected(),
    );

    builder.extra.push(extra_dashboard(config)?);

    Ok(())
}

fn count_panel(
    config: &TeamConfig,
    components: &[&str],
    application: Application,
    channel: ReleaseChannel,
) -> Panel {
    if application == Application::Ios && channel == ReleaseChannel::Nightly {
        // iOs doesn't have a nightly
        return TextPanel::new("## N/A").into();
    }

    let mut query = Query {
        prep_statements: error_type_re_prep_statements(components),
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
    query.add_count_per_day_column("COUNT(*)", "error_count");

    TimeSeriesPanel::new_grouped(
        application.display_name(channel),
        query.sql(),
        "error_count",
        "error_type",
    )
    .with_axis_label("errors / day")
    .with_link(
        "Error list",
        UrlBuilder::new_dashboard(config.dashboard_slug("errors"))
            .with_time_range_param()
            .with_param("var-application", application.slug())
            .with_param("var-channel", channel.to_string())
            .with_param("var-error_type", "${__data.fields[\"error_type\"]}")
            .build(),
    )
    .into()
}

fn extra_dashboard(config: &TeamConfig) -> Result<Dashboard> {
    let mut builder = DashboardBuilder::new(config, "errors", "Error List");
    builder.add_application_variable();
    builder.add_channel_variable();
    builder.add_variable(error_type_variable());
    builder.add_variable(version_variable());
    builder.add_variable(build_date_variable());
    builder.add_variable(filter_variable(
        "Architecture",
        "architecture",
        "architecture",
    ));
    builder.add_variable(filter_variable(
        "Device Manufacturer",
        "device_manufacturer",
        "device_manufacturer",
    ));
    builder.add_variable(filter_variable(
        "Device Model",
        "device_model",
        "device_model",
    ));
    builder.add_variable(filter_variable("Locale", "locale", "locale"));
    builder.add_variable(filter_variable("OS", "os", "os"));
    builder.add_variable(filter_variable("OS Version", "os_version", "os_version"));
    builder.add_variable(TextBoxVariable::new("Search", "search"));

    builder.add_panel_full(error_list_count_panel());
    builder.add_panel_full(error_list_log_panel());

    Ok(builder.dashboard)
}

pub fn error_type_variable() -> QueryVariable {
    let subquery = error_subquery().as_subquery();
    let query = format!(
        "\
SELECT DISTINCT error_type
FROM {subquery}
WHERE submission_timestamp > TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL 3 day)
    AND error_type IS NOT NULL
    AND error_type <> ''
ORDER BY error_type"
    );

    QueryVariable::new("Error type", "error_type", query)
}

pub fn version_variable() -> QueryVariable {
    let subquery = error_subquery().as_subquery();
    let query = format!("\
SELECT 'All' as text, '' as value
UNION ALL
SELECT version as text, version as value
FROM (
    SELECT DISTINCT CAST(mozfun.norm.extract_version(app_display_version, 'major') AS STRING) as version
    FROM {subquery}
    WHERE submission_timestamp > TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL 3 day)
        AND mozfun.norm.extract_version(app_display_version, 'major') IS NOT NULL
    ORDER BY 1 DESC
)");

    QueryVariable::new("Version", "version", query)
        .with_sort(VariableSortOrder::AlphabeticalDescending)
}

pub fn build_date_variable() -> QueryVariable {
    let subquery = error_subquery().as_subquery();
    let query = format!(
        "\
SELECT 'None' as text, '' as value
UNION ALL
SELECT build_date as text, build_date as value
FROM (
    SELECT DISTINCT SUBSTR(build_date, 0, 10) as build_date
    FROM {subquery}
    WHERE submission_timestamp > TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL 3 day)
    ORDER BY 1 DESC
)"
    );

    QueryVariable::new("Group by build date", "build_date", query)
        .with_sort(VariableSortOrder::AlphabeticalDescending)
}

fn filter_variable(label: &str, filter_name: &str, column_name: &str) -> QueryVariable {
    let subquery = error_subquery().as_subquery();
    let query = format!(
        "\
SELECT 'All' as text, '' as value
UNION ALL
SELECT value as text, value
FROM (
    SELECT DISTINCT {column_name} as value
    FROM {subquery}
    WHERE submission_timestamp > TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL 3 day)
    ORDER BY 1 DESC
)"
    );

    QueryVariable::new(label, filter_name, query)
        .with_sort(VariableSortOrder::AlphabeticalCaseInsensitiveAscending)
        .with_include_all()
        .with_all_option()
}

fn filter_where_clause(name: &str) -> String {
    format!("${{{name}:sqlstring}} = '' OR ${{{name}:sqlstring}} = '<all>' OR {name} = ${{{name}:sqlstring}}")
}

fn error_list_count_panel() -> Panel {
    let mut query = Query {
        select: vec![
            "$__timeGroup(submission_timestamp, $__interval) as time".into(),
            "IF('${build_date}' = '', '', IF(build_date < '${build_date}', '< ${build_date}', '>= ${build_date}')) as build_date".into(),
            "COUNT(DISTINCT client_id) as client_count".into(),
        ],
        where_: vec![
            "error_type='${error_type}'".into(),
            "$__timeFilter(submission_timestamp)".into(),
            "normalized_channel = '${channel}'".into(),
            "'${version}' = '' OR version = CAST('${version}' AS NUMERIC)".into(),
            "'${search}' = '' OR (details LIKE '%${search}%') OR client_id = '${search}'".into(),
            filter_where_clause("architecture"),
            filter_where_clause("device_manufacturer"),
            filter_where_clause("device_model"),
            filter_where_clause("locale"),
            filter_where_clause("os"),
            filter_where_clause("os_version"),
        ],
        from: error_subquery().as_subquery(),
        group_by: Some("1, 2".into()),
        order_by: Some("1 ASC, 2 ASC".into()),
        ..Query::default()
    };
    query.add_count_per_day_column("COUNT(*)", "error_count");

    TimeSeriesPanel::new_with_timeseries_query("", query.sql())
        .with_axis_label("errors / day")
        .into()
}

fn error_list_log_panel() -> Panel {
    let query = Query {
        select: vec![
            "CONCAT(error_type, ': ', details) as message".into(),
            "error_type".into(),
            "details".into(),
            "ARRAY_TO_STRING(breadcrumbs, '\\n') as breadcrumbs".into(),
            "client_id".into(),
            "app_display_version".into(),
            "architecture".into(),
            "build_date".into(),
            "device_manufacturer".into(),
            "device_model".into(),
            "locale".into(),
            "os".into(),
            "os_version".into(),
            "submission_timestamp".into(),
        ],
        where_: vec![
            "error_type='${error_type}'".into(),
            "$__timeFilter(submission_timestamp)".into(),
            "normalized_channel = '${channel}'".into(),
            // If the group by build date var is set, then only show logs after that build date.
            // The typical use case is wanted to know if a change fixed an error.
            // In that case, it's usually better to only see the logs after the change.
            "IF('${build_date}' = '', true, build_date >= '${build_date}')".into(),
            "'${version}' = '' OR version = CAST('${version}' AS NUMERIC)".into(),
            "'${search}' = '' OR (details LIKE '%${search}%') OR client_id = '${search}'".into(),
            filter_where_clause("architecture"),
            filter_where_clause("device_manufacturer"),
            filter_where_clause("device_model"),
            filter_where_clause("locale"),
            filter_where_clause("os"),
            filter_where_clause("os_version"),
        ],
        from: error_subquery().as_subquery(),
        order_by: Some("submission_timestamp DESC".into()),
        limit: Some(1000),
        ..Query::default()
    };

    LogPanel::new("Error list", query.sql())
        .with_details()
        .into()
}

// Select everything from `rust_component_errors_error_type`, but "flatten" the column names.
//
// This means `error_type` instead of `metrics.string.rust_component_errors_error_type`, which is
// needed to make the filters work.
fn error_subquery() -> Union {
    let mut base_query = Query {
        select: vec![
            "SUBSTR(client_info.build_date, 0, 10) as build_date".into(),
            "mozfun.norm.extract_version(client_info.app_display_version, 'major') as version"
                .into(),
            "metrics.string.rust_component_errors_error_type as error_type".into(),
            "metrics.string.rust_component_errors_details as details".into(),
            "metrics.string_list.rust_component_errors_breadcrumbs as breadcrumbs".into(),
            "client_info.client_id as client_id".into(),
            "normalized_channel".into(),
        ],
        ..Query::default()
    };
    base_query.add_standard_glean_columns();
    Union {
        queries: vec![
            Query {
                from: "mozdata.fenix.rust_component_errors".into(),
                where_: vec!["'${application}' = 'firefox_android'".into()],
                ..base_query.clone()
            },
            Query {
                from: "mozdata.firefox_ios.rust_component_errors".into(),
                where_: vec!["'${application}' = 'firefox_ios'".into()],
                ..base_query.clone()
            },
            Query {
                from: "mozdata.firefox_desktop.rust_component_errors".into(),
                where_: vec!["'${application}' = 'firefox_desktop'".into()],
                ..base_query.clone()
            },
        ],
        ..Union::default()
    }
}

/// Bigquery statements to define the `error_type_re` variable
///
/// This is a bigquery variable created from the `rust_component_errors_filter` grafana variable.
/// We use it as a regex to match error pings against.
fn error_type_re_prep_statements(components: &[&str]) -> Vec<String> {
    // `error_type_re` variable;
    let mut query_parts = vec![];
    query_parts.push("SELECT CASE value".into());
    for c in components.iter() {
        query_parts.push(format!("WHEN '{c}' THEN '^{c}-'"));
    }
    query_parts.push("END".into());
    query_parts
        .push("FROM UNNEST(SPLIT('${rust_component_errors_filter:csv}', ',')) as value".into());
    vec![
        "DECLARE error_type_re STRING".into(),
        format!(
            "SET error_type_re = ARRAY_TO_STRING(ARRAY({}), '|')",
            query_parts.join(" ")
        ),
    ]
}

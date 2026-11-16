/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use anyhow::Result;

use crate::{
    config::*,
    metrics,
    metrics::sync::SyncEngine,
    schema::{Dashboard, DashboardBuilder, TeamDashboards, TeamDashboardsBuilder},
};

pub fn dashboards() -> Result<TeamDashboards> {
    let config = TeamConfig { team_name: "SYNC" };
    let mut builder = TeamDashboardsBuilder::new(&config);

    metrics::sync::build_dashboards(
        &config,
        &mut builder,
        &[
            SyncEngine::Addresses,
            SyncEngine::Bookmarks,
            SyncEngine::CreditCards,
            SyncEngine::History,
            SyncEngine::Logins,
            SyncEngine::Prefs,
            SyncEngine::RustLogins,
            SyncEngine::Tabs,
            SyncEngine::WebextStorage,
        ],
    )?;

    metrics::rust_component_errors::build_dashboards(
        &config,
        &mut builder,
        &[
            "autofill",
            "fxa-client",
            "logins",
            "places",
            "remote-settings",
            "prefs",
            "tabs",
            "webext-storage",
        ],
    )?;

    builder
        .extra
        .push(places_run_maintenance_extra_dashboard(&config)?);

    metrics::DistributionMetric {
        display_name: "Places run_maintenance() time",
        ping: "metrics",
        category: "places_manager",
        metric: "run_maintenance_time",
        applications: vec![Application::Android],
        axis_label: "time",
        unit: Some(Unit::Milliseconds),
        value_divisor: Some(1_000_000),
        link_to: Some(builder.extra_dashboard_uid("maintenance-times")?),
    }
    .build_dashboard(&mut builder.main)?;

    metrics::EventsMetric {
        display_name: "Logins key regeneration (Android)",
        ping: "metrics",
        category: "logins_store",
        metrics: vec![
            "key_regenerated_lost",
            "key_regenerated_corrupt",
            "key_regenerated_other",
        ],
        applications: vec![Application::Android],
        options: metrics::EventsOptions::default(),
    }
    .build_dashboard(&mut builder.main)?;

    metrics::EventsMetric {
        display_name: "Logins key regeneration (iOS)",
        ping: "metrics",
        category: "logins_store_key_regeneration",
        metrics: vec!["lost", "corrupt", "other"],
        applications: vec![Application::Ios],
        options: metrics::EventsOptions::default(),
    }
    .build_dashboard(&mut builder.main)?;

    metrics::EventsMetric {
        display_name: "Credit Card key regeneration",
        ping: "metrics",
        category: "credit_card_key_regeneration",
        metrics: vec!["keychain_data_lost", "lost", "corrupt", "other"],
        applications: vec![Application::Ios],
        options: metrics::EventsOptions::default(),
    }
    .build_dashboard(&mut builder.main)?;

    metrics::EventsMetric {
        display_name: "Logins undecryptable count",
        ping: "metrics",
        category: "logins_store",
        metrics: vec![
            "local_undecryptable_deleted",
            "mirror_undecryptable_deleted",
        ],
        applications: vec![Application::Android],
        options: metrics::EventsOptions {
            unique_user_counts: true,
        },
    }
    .build_dashboard(&mut builder.main)?;

    metrics::EventsMetric {
        display_name: "Credit Card undecryptable count",
        ping: "metrics",
        category: "user_credit_cards",
        metrics: vec!["undecryptable_count"],
        applications: vec![Application::Ios],
        options: metrics::EventsOptions {
            unique_user_counts: true,
        },
    }
    .build_dashboard(&mut builder.main)?;

    Ok(builder.finish())
}

fn places_run_maintenance_extra_dashboard(config: &TeamConfig) -> Result<Dashboard> {
    let mut builder = DashboardBuilder::new(config, "maintenance-times", "Run Maintenance Times");

    metrics::DistributionMetric {
        display_name: "Places run_maintenance_chk_pnt_time() time",
        ping: "metrics",
        category: "places_manager",
        metric: "run_maintenance_chk_pnt_time",
        axis_label: "time",
        unit: Some(Unit::Milliseconds),
        value_divisor: Some(1_000_000),
        applications: vec![Application::Android],
        link_to: None,
    }
    .build_dashboard(&mut builder)?;

    metrics::DistributionMetric {
        display_name: "Places run_maintenance_optimize_time() time",
        ping: "metrics",
        category: "places_manager",
        metric: "run_maintenance_optimize_time",
        axis_label: "time",
        unit: Some(Unit::Milliseconds),
        value_divisor: Some(1_000_000),
        applications: vec![Application::Android],
        link_to: None,
    }
    .build_dashboard(&mut builder)?;

    metrics::DistributionMetric {
        display_name: "Places run_maintenance_prune_time() time",
        ping: "metrics",
        category: "places_manager",
        metric: "run_maintenance_prune_time",
        axis_label: "time",
        unit: Some(Unit::Milliseconds),
        value_divisor: Some(1_000_000),
        applications: vec![Application::Android],
        link_to: None,
    }
    .build_dashboard(&mut builder)?;

    metrics::DistributionMetric {
        display_name: "Places run_maintenance_vacuum_time() time",
        ping: "metrics",
        category: "places_manager",
        metric: "run_maintenance_vacuum_time",
        axis_label: "time",
        unit: Some(Unit::Milliseconds),
        value_divisor: Some(1_000_000),
        applications: vec![Application::Android],
        link_to: None,
    }
    .build_dashboard(&mut builder)?;

    Ok(builder.dashboard)
}

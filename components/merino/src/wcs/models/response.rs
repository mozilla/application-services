/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, uniffi::Record)]
pub struct WcsTeamStanding {
    pub wins: i32,
    pub losses: i32,
    pub draws: i32,
    pub points: i32,
}

#[derive(Clone, Debug, Deserialize, uniffi::Record)]
pub struct WcsTeamInfo {
    pub key: String,
    pub global_team_id: i64,
    pub name: String,
    pub region: String,
    pub colors: Vec<String>,
    pub icon_url: Option<String>,
    pub group: String,
    pub eliminated: bool,
    pub standing: WcsTeamStanding,
}

#[derive(Clone, Debug, Deserialize, uniffi::Record)]
pub struct WcsEventInfo {
    pub date: String,
    pub global_event_id: i64,
    pub home_team: WcsTeamInfo,
    pub away_team: WcsTeamInfo,
    pub period: String,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
    pub home_extra: Option<i32>,
    pub away_extra: Option<i32>,
    pub home_penalty: Option<i32>,
    pub away_penalty: Option<i32>,
    /// Elapsed match time in minutes; extra time shown as e.g. `"90+3"`.
    pub clock: String,
    /// UTC Unix timestamp of the last data update.
    pub updated: i64,
    /// Human-readable status, e.g. `"In Progress"`, `"Final"`, `"Scheduled"`.
    pub status: String,
    /// Simplified status: `"live"`, `"past"`, or `"scheduled"`.
    pub status_type: String,
    pub query: Option<String>,
    pub sport: String,
}

#[derive(Clone, Debug, Deserialize, uniffi::Record)]
pub struct WcsLiveMatchesResponse {
    pub matches: Vec<WcsEventInfo>,
}

/// Response from `GET /api/v1/wcs/matches`, bucketed by match timing relative to the target date.
#[derive(Clone, Debug, Deserialize, uniffi::Record)]
pub struct WcsMatchesResponse {
    /// Matches that have already ended.
    pub previous: Vec<WcsEventInfo>,
    /// Matches currently in progress or starting soon.
    pub current: Vec<WcsEventInfo>,
    /// Upcoming matches.
    pub next: Vec<WcsEventInfo>,
}

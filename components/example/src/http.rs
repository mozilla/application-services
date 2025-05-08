/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Http client example
//!
//! This creates a TODO item from a GitHub issue.  This is kind-of silly, but you should still be
//! able to use the code as a starting point for your component's HTTP requirements.

use serde::Deserialize;
use url::Url;
use viaduct::{header_names, Request};

use crate::{Result, TodoItem};

/// Http client struct
///
/// This example client has no fields. A real component would store things like cached responses,
/// auth data, etc.
pub struct HttpClient {}

impl HttpClient {
    pub fn new() -> Self {
        Self {}
    }

    pub fn fetch_todo_from_gh_issue(&self, name: &str, issue_id: &str) -> Result<TodoItem> {
        // Use `components/viaduct` library to make HTTP requests.  See that component for details
        // on the API.
        let url = Url::parse(&format!(
            "https://api.github.com/repos/mozilla/application-services/issues/{issue_id}"
        ))?;
        crate::error::trace!("making request: {url}");
        let request = Request::get(url)
            .header(header_names::ACCEPT, "application/vnd.github+json")?
            .header(
                header_names::USER_AGENT,
                "Application-services example client",
            )?
            .header("X-GitHub-Api-Version", "2022-11-28")?;
        let response = request.send()?;
        crate::error::trace!("response: {}", response.text());
        // Response.json() deserializes the response using Serde.
        let issue: GithubIssue = response.json()?;
        Ok(TodoItem {
            name: name.into(),
            description: issue.title,
            url: issue.html_url,
            completed: matches!(issue.state, GithubIssueState::Closed),
        })
    }
}

/// You probably want to use `serde` to deserialize JSON responses into Rust structs.
///
/// This is mostly straightforward and intuitive.  Check out https://serde.rs/ for details.

#[derive(Deserialize)]
struct GithubIssue {
    title: String,
    html_url: String,
    state: GithubIssueState,
}

#[derive(Deserialize)]
enum GithubIssueState {
    // Use `rename` when the name in the Rust struct doesn't match the name in the JSON data.
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "closed")]
    Closed,
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error::RestmailClientError;
use error_support::{info, warn};
use serde_json::Value as EmailJson;
use url::Url;
use viaduct::Request;

mod error;

type Result<T> = std::result::Result<T, RestmailClientError>;

/// For a given restmail email, find the first email that satisfies the given predicate.
/// If no email is found, this function sleeps for a few seconds then tries again, up
/// to `max_tries` times.
pub fn find_email<F>(email: &str, predicate: F, max_tries: u8) -> Result<EmailJson>
where
    F: Fn(&EmailJson) -> bool,
{
    let mail_url = url_for_email(email)?;
    info!("Checking {} up to {} times.", email, max_tries);
    for i in 0..max_tries {
        let resp: Vec<serde_json::Value> = Request::get(mail_url.clone()).send()?.json()?;
        let mut matching_emails: Vec<serde_json::Value> =
            resp.into_iter().filter(|email| predicate(email)).collect();

        if matching_emails.is_empty() {
            info!(
                "Failed to find matching email. Waiting {} seconds and retrying.",
                i + 1
            );
            std::thread::sleep(std::time::Duration::from_secs((i + 1).into()));
            continue;
        }

        if matching_emails.len() > 1 {
            info!(
                "Found {} emails that applies (taking latest)",
                matching_emails.len()
            );
            matching_emails.sort_by(|a, b| {
                let a_time = a["receivedAt"].as_u64();
                let b_time = b["receivedAt"].as_u64();
                match (a_time, b_time) {
                    (Some(a_time), Some(b_time)) => b_time.cmp(&a_time),
                    _ => {
                        warn!("Could not de-serialize receivedAt for at least one of the emails.");
                        std::cmp::Ordering::Equal
                    }
                }
            })
        }
        return Ok(matching_emails[0].clone());
    }
    info!("Error: Failed to find email after {} tries!", max_tries);
    Err(RestmailClientError::HitRetryMax)
}

pub fn clear_mailbox(email: &str) -> Result<()> {
    let mail_url = url_for_email(email)?;
    info!("Clearing restmail for {}.", email);
    Request::delete(mail_url).send()?;
    Ok(())
}

fn username_from_email(email: &str) -> Result<String> {
    let user = email.replace("@restmail.net", "");
    if user.len() == email.len() {
        return Err(RestmailClientError::NotARestmailEmail);
    }
    Ok(user)
}

fn url_for_email(email: &str) -> Result<Url> {
    let restmail_user = username_from_email(email)?;
    let path = format!("/mail/{}", restmail_user);
    Ok(Url::parse("https://restmail.net")?.join(&path)?)
}

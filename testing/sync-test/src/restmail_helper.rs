/* Any copyright is dedicated to the Public Domain.
http://creativecommons.org/publicdomain/zero/1.0/ */

use serde_json::Value as EmailJson;
use url::Url;
use viaduct::Request;

fn restmail_username(email: &str) -> String {
    let user = email.replace("@restmail.net", "");
    if user.len() == email.len() {
        panic!("Not a restmail account (doesn't end with @restmail.net)");
    }
    user
}

fn restmail_url(email: &str) -> Url {
    let restmail_user = restmail_username(email);
    let path = format!("/mail/{}", restmail_user);
    Url::parse("https://restmail.net")
        .unwrap()
        .join(&path)
        .unwrap()
}

/// For a given restmail email,
/// find the first email that satisfies the given predicate.
pub fn find_email<F>(email: &str, predicate: F) -> EmailJson
where
    F: Fn(&EmailJson) -> bool,
{
    let max_tries = 10;
    let mail_url = restmail_url(email);
    log::info!("Checking {} up to {} times.", email, max_tries);
    for i in 0..max_tries {
        let resp: Vec<serde_json::Value> = Request::get(mail_url.clone())
            .send()
            .unwrap()
            .json()
            .unwrap();
        let mut matching_emails: Vec<serde_json::Value> =
            resp.into_iter().filter(|email| predicate(email)).collect();

        if matching_emails.is_empty() {
            log::info!(
                "Failed to find matching email. Waiting {} seconds and retrying.",
                i + 1
            );
            std::thread::sleep(std::time::Duration::from_secs(i + 1));
            continue;
        }

        if matching_emails.len() > 1 {
            log::info!(
                "Found {} emails that applies (taking latest)",
                matching_emails.len()
            );
            matching_emails.sort_by(|a, b| {
                let a_time = a["receivedAt"].as_u64().unwrap();
                let b_time = b["receivedAt"].as_u64().unwrap();
                b_time.cmp(&a_time)
            })
        }

        return matching_emails[0].clone();
    }
    log::info!("Error: Failed to find email after {} tries!", max_tries);
    panic!("Hit retry max!");
}

pub fn clear_mailbox(email: &str) {
    let mail_url = restmail_url(email);
    log::info!("Clearing restmail for {}.", email);
    Request::delete(mail_url).send().unwrap();
}

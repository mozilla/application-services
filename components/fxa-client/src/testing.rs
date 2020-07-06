/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    error::{ErrorKind, Result},
    Config,
};
use rc_crypto::rand;
use serde_json::json;
use viaduct::Request;

pub struct TempAccountDetails {
    pub email: String,
    pub password: String,
}

pub fn create_temp_account(
    content_url: &str,
    client_id: &str,
    redirect_uri: &str,
    token_server_url_override: Option<&str>,
) -> Result<TempAccountDetails> {
    let config = Config::new_with_token_server_override(
        content_url,
        client_id,
        redirect_uri,
        token_server_url_override,
    );

    let account_details = rand_account_details()?;

    restmail_client::clear_mailbox(&account_details.email)?;

    // XXX: This needs to be consolidated with `auth.rs` in sync-test,
    // to be done in a follow-up.
    let create_endpoint = config.auth_url_path("v1/account/create")?;
    let body = json!({
        "email": &account_details.email,
        "authPW": crate::auth::auth_pwd(&account_details.email, &account_details.password)?,
        "service": &config.client_id,
    });
    let req = Request::post(create_endpoint).json(&body).send()?;
    let resp: serde_json::Value = req.json()?;
    let uid = resp["uid"]
        .as_str()
        .ok_or_else(|| ErrorKind::UnrecoverableServerError("no uid in response"))?;

    verify_account(&account_details.email, &config, &uid)?;

    Ok(account_details)
}

pub fn destroy_account(
    content_url: &str,
    client_id: &str,
    redirect_uri: &str,
    token_server_url_override: Option<&str>,
    email: &str,
    password: &str,
) -> Result<()> {
    let config = Config::new_with_token_server_override(
        content_url,
        client_id,
        redirect_uri,
        token_server_url_override,
    );

    let destroy_endpoint = config.auth_url_path("v1/account/destroy").unwrap();
    let body = json!({
        "email": email,
        "authPW": crate::auth::auth_pwd(&email, &password).unwrap()
    });
    Request::post(destroy_endpoint)
        .json(&body)
        .send()?
        .require_success()?;
    Ok(())
}

// TODO: Copy pasta from auth.rs.
fn verify_account(email_in: &str, config: &Config, uid: &str) -> Result<()> {
    let verification_email = restmail_client::find_email(
        email_in,
        |email| email["headers"]["x-uid"] == uid && email["headers"]["x-template-name"] == "verify",
        10,
    )?;
    let code = verification_email["headers"]["x-verify-code"]
        .as_str()
        .ok_or_else(|| ErrorKind::IllegalState("No verify code in headers"))?;
    let body = json!({
        "uid": uid,
        "code": code,
    });
    crate::auth::send_verification(&config, body)?.require_success()?;
    Ok(())
}

fn rand_account_details() -> Result<TempAccountDetails> {
    let username = rand_str()?;
    let email = format!("{}@restmail.net", username);
    // Use the same as username so we can inspect the email manually easily.
    let password = username;

    Ok(TempAccountDetails { email, password })
}

fn rand_str() -> Result<String> {
    let mut buf = vec![0; 16];
    rand::fill(&mut buf)?;
    Ok(base64::encode_config(buf, base64::URL_SAFE_NO_PAD))
}

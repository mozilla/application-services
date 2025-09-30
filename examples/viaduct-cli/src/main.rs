/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use clap::{Parser, ValueEnum};

use viaduct::{header_names, Client, ClientSettings, Method, Request, Response, Result};

#[derive(Debug, Parser)]
#[command(about, long_about = None)]
struct Cli {
    /// Backend style
    #[arg(short, long)]
    backend: Option<BackendStyle>,
    /// Make a Post request
    #[arg(short, long)]
    post_request: bool,
    /// Set a request timeout (ms)
    #[arg(short, long)]
    timeout: Option<u64>,
}

#[derive(Clone, Debug, ValueEnum)]
enum BackendStyle {
    // New backend
    New,
    // Bridged backend: initialize the new backend, but use the old API
    Bridged,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    println!("{cli:?}");
    let backend_style = cli.backend.unwrap_or(BackendStyle::New);
    let req = if cli.post_request {
        make_post_request()?
    } else {
        make_request()?
    };
    match backend_style {
        BackendStyle::New => {
            viaduct_hyper::init_backend_hyper()?;
            let settings = ClientSettings {
                timeout: cli.timeout.unwrap_or(0) as u32,
                ..ClientSettings::default()
            };
            let client = Client::new(settings);
            print_response(client.send_sync(req))
        }
        BackendStyle::Bridged => {
            viaduct_hyper::init_backend_hyper()?;
            if let Some(t) = cli.timeout {
                set_old_global_timeout(t);
            }
            print_response(req.send())
        }
    }

    Ok(())
}

fn set_old_global_timeout(timeout: u64) {
    let mut s = viaduct::settings::GLOBAL_SETTINGS.write();
    s.connect_timeout = Some(std::time::Duration::from_millis(timeout));
    s.read_timeout = Some(std::time::Duration::from_millis(timeout));
}

fn make_request() -> Result<Request> {
    let url = url::Url::parse("https://httpbun.org/anything")?;
    let mut req = Request::new(Method::Get, url);
    req = req.header(header_names::USER_AGENT, "viaduct-cli")?;
    Ok(req)
}

#[derive(serde::Serialize)]
struct TestPostData {
    data: String,
    more_data: String,
}

fn make_post_request() -> Result<Request> {
    let url = url::Url::parse("https://httpbun.org/anything")?;
    let mut req = Request::new(Method::Post, url);
    req = req.header(header_names::USER_AGENT, "viaduct-cli")?;
    let req = req.json(&TestPostData {
        data: "Hello".to_owned(),
        more_data: "World".to_owned(),
    });
    Ok(req)
}

fn print_response(response: Result<Response>) {
    match response {
        Ok(response) => {
            println!("got response");
            println!("status: {}", response.status);
            println!("\nHEADERS:");
            for h in response.headers {
                println!("{}: {}", h.name, h.value);
            }
            println!("\nRESPONSE:");
            println!("{}", String::from_utf8(response.body).unwrap());
        }
        Err(e) => {
            println!("error: {e}");
        }
    }
}

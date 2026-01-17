/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use clap::{Parser, Subcommand, ValueEnum};
use url::Url;
#[cfg(feature = "ohttp")]
use viaduct::{configure_ohttp_channel, OhttpConfig};
use viaduct::{header_names, Client, ClientSettings, Method, Request, Response, Result};

#[derive(Debug, Parser)]
#[command(about, long_about = None)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Backend style
    #[arg(short, long)]
    backend: Option<BackendStyle>,

    /// Set a request timeout (ms)
    #[arg(short, long)]
    timeout: Option<u64>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Make a simple HTTP request
    Request {
        /// Make a Post request
        #[arg(short, long)]
        post: bool,
    },
    /// Test OHTTP flow with a relay
    #[cfg(feature = "ohttp")]
    Ohttp {
        /// OHTTP relay URL
        #[arg(
            long,
            default_value = "https://mozilla-ohttp-relay-test.edgecompute.app/"
        )]
        relay_url: String,

        /// Gateway host for fetching encryption keys
        #[arg(
            long,
            default_value = "stage.ohttp-gateway.nonprod.webservices.mozgcp.net"
        )]
        gateway_host: String,

        /// Channel name for OHTTP
        #[arg(long, default_value = "merino")]
        channel: String,
    },
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

    // Initialize logging
    cli_support::init_logging_with(if cli.verbose {
        "viaduct=trace"
    } else {
        "viaduct=info"
    });

    println!("{cli:?}");
    let backend_style = cli.backend.unwrap_or(BackendStyle::New);

    match cli.command {
        Commands::Request { post } => {
            let req = if post {
                make_post_request()?
            } else {
                make_request()?
            };

            match backend_style {
                BackendStyle::New => {
                    viaduct_hyper::viaduct_init_backend_hyper()?;
                    let settings = ClientSettings {
                        timeout: cli.timeout.unwrap_or(0) as u32,
                        ..ClientSettings::default()
                    };
                    let client = Client::new(settings);
                    print_response(client.send_sync(req));
                }
                BackendStyle::Bridged => {
                    viaduct_hyper::viaduct_init_backend_hyper()?;
                    if let Some(t) = cli.timeout {
                        set_old_global_timeout(t);
                    }
                    print_response(req.send());
                }
            }
        }
        #[cfg(feature = "ohttp")]
        Commands::Ohttp {
            relay_url,
            gateway_host,
            channel,
        } => {
            return run_ohttp_example(relay_url, gateway_host, channel, backend_style);
        }
    }

    Ok(())
}

#[cfg(feature = "ohttp")]
fn run_ohttp_example(
    relay_url: String,
    gateway_host: String,
    channel: String,
    backend_style: BackendStyle,
) -> Result<()> {
    // Step 1: Initialize viaduct backend
    println!("Initializing viaduct backend...");

    match backend_style {
        BackendStyle::New => {
            viaduct_hyper::viaduct_init_backend_hyper()?;
        }
        BackendStyle::Bridged => {
            println!("OHTTP is not compatible with the bridged backend. Use --backend=new or omit the backend parameter.");
            return Ok(());
        }
    }

    println!("Backend initialized successfully");

    // Step 2: Configure the OHTTP channel
    println!("Configuring OHTTP channel...");
    configure_ohttp_channel(
        channel.clone(),
        OhttpConfig {
            relay_url,
            gateway_host,
        },
    )?;
    println!("OHTTP channel configured");

    // Step 3: Create OHTTP client
    println!("Creating OHTTP client...");
    let client = Client::with_ohttp_channel(&channel, ClientSettings::default())?;
    println!("OHTTP client created");

    // Step 4: Make request
    println!("Creating request...");
    let request = Request::get(Url::parse("https://merino.services.mozilla.com/docs")?);
    println!("Sending request...");
    let response = client.send_sync(request)?;

    // Step 5: Handle response
    println!("Response received!");
    println!("Status: {}", response.status);
    println!("Body: {}", String::from_utf8_lossy(&response.body));

    Ok(())
}

fn set_old_global_timeout(timeout: u64) {
    let mut s = viaduct::settings::GLOBAL_SETTINGS.write();
    s.connect_timeout = Some(std::time::Duration::from_millis(timeout));
    s.read_timeout = Some(std::time::Duration::from_millis(timeout));
}

fn make_request() -> Result<Request> {
    let url = Url::parse("https://httpbun.org/anything")?;
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
    let url = Url::parse("https://httpbun.org/anything")?;
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

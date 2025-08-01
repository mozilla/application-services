/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use clap::Parser;
use url::Url;
use viaduct::Request;
use viaduct_dev::{use_dev_backend, Result};

#[derive(Parser)]
struct Args {
    url: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let url = Url::parse(&args.url)?;
    use_dev_backend();
    let resp = Request::get(url.clone()).send()?;
    if resp.url != url {
        println!("Redirected URL: {}", resp.url);
    }
    println!("status: {}", resp.status);
    for header in resp.headers.into_vec() {
        println!("{}: {}", header.name(), header.value());
    }
    println!();
    println!("{}", String::from_utf8_lossy(&resp.body));

    Ok(())
}

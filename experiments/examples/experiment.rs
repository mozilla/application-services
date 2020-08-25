// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;
use experiments::{AppContext, Experiments};
fn main() -> Result<()> {
    use env_logger::Env;
    // We set the logging level to be `warn` here, meaning that only
    // logs of `warn` or higher will be actually be shown, any other
    // error will be omitted
    // To manually set the log level, you can set the `RUST_LOG` environment variable
    // Possible values are "info", "debug", "warn" and "error"
    // Check [`env_logger`](https://docs.rs/env_logger/) for more details
    env_logger::from_env(Env::default().default_filter_or("warn")).init();
    viaduct_reqwest::use_reqwest_backend();
    let exp = Experiments::new(
        "messaging-experiments".to_string(),
        AppContext::default(),
        "../target/mydb",
        None,
    )
    .unwrap();
    exp.get_active_experiments()
        .iter()
        .for_each(|e| println!("Experiment: {}", e.slug));
    Ok(())
}

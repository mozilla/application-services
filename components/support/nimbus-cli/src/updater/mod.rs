// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod taskcluster;

use std::io::Write;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

pub(crate) fn check_for_update() {
    if std::env::var("NIMBUS_CLI_SUPPRESS_UPDATE_CHECK").is_ok() {
        return;
    }
    if let Some((curr, next)) = taskcluster::check_taskcluster_for_update() {
        _ = print_update_instructions(&curr, &next);
    }
}

fn print_update_instructions(curr: &str, next: &str) -> std::io::Result<()> {
    let mut stderr = StandardStream::stderr(ColorChoice::Auto);

    stderr.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
    writeln!(&mut stderr, "An update is available: {} --> {}", curr, next)?;

    cfg_if::cfg_if! {
        if #[cfg(windows)] {
            writeln!(&mut stderr, "To update follow the instructions at https://experimenter.info/nimbus-cli/install")?;
        } else {
            writeln!(&mut stderr, "Up update, run this command:")?;
            stderr.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
            writeln!(&mut stderr, "  curl https://raw.githubusercontent.com/mozilla/application-services/main/install-nimbus-cli.sh | bash")?;
        }
    }

    stderr.reset()?;

    Ok(())
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod taskcluster;

use console::Term;

pub(crate) fn check_for_update() {
    if std::env::var("NIMBUS_CLI_SUPPRESS_UPDATE_CHECK").is_ok() {
        return;
    }
    taskcluster::check_taskcluster_for_update(|curr, next| {
        let term = Term::stderr();
        let txt_style = term.style().green();
        let cmd_style = term.style().yellow();

        _ = term.write_line(&format!(
            "{}",
            txt_style.apply_to(format!("An update is available: {} --> {}", curr, next))
        ));

        _ = if std::env::consts::OS != "windows" {
            term.write_line(&format!("{}\n{}",
                txt_style.apply_to("To update, run this command:"),
                cmd_style.apply_to("  curl https://raw.githubusercontent.com/mozilla/application-services/main/install-nimbus-cli.sh | bash")
            ))
        } else {
            term.write_line(&format!("{}",
                txt_style.apply_to("To update follow the instructions at https://experimenter.info/nimbus-cli/install")
            ))
        };
    });
}

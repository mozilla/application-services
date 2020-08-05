// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;
use experiments::Experiments;
fn main() -> Result<()> {
    let exp = Experiments::new();
    exp.get_experiments();
    Ok(())
}

/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

pub(crate) mod helpers;
mod test_defaults;
mod test_enrollment;
mod test_enrollment_bw_compat;
mod test_evaluator;
mod test_lib_bw_compat;
mod test_sampling;
mod test_schema;

#[cfg(feature = "stateful")]
mod stateful {
    mod test_behavior;
    mod test_enrollment;
    mod test_evaluator;
    mod test_gecko_prefs;
    mod test_nimbus;
    mod test_persistence;
    mod test_targeting;
    mod test_updating;

    mod client {
        mod test_http_client;
        mod test_null_client;
    }
}

#[cfg(not(feature = "stateful"))]
mod stateless {
    mod test_cirrus_client;
}

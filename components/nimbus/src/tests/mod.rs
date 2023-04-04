/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[cfg(feature = "stateful")]
mod test_behavior;
mod test_defaults;
mod test_enrollment;
mod test_enrollment_bw_compat;
mod test_evaluator;
mod test_lib_bw_compat;
mod test_lib_schema_deserialization;
#[cfg(feature = "stateful")]
mod test_nimbus;
#[cfg(feature = "stateful")]
mod test_persistence;
mod test_sampling;
#[cfg(feature = "stateful")]
mod test_updating;
mod test_versioning;

#[cfg(feature = "stateful")]
mod client {
    mod test_http_client;
    mod test_null_client;
}

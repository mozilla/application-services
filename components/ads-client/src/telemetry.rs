/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::any::Any;

pub trait Telemetry {
    fn record(&self, event: &dyn Any);

    // Shuts down the telemetry wrapper by replacing it with a noop.
    // Future telemetry records will not record anything.
    fn shutdown(&self);
}

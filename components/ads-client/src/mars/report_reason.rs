/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReportReason {
    Inappropriate,
    NotInterested,
    SeenTooManyTimes,
}

impl ReportReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReportReason::Inappropriate => "inappropriate",
            ReportReason::NotInterested => "not_interested",
            ReportReason::SeenTooManyTimes => "seen_too_many_times",
        }
    }
}

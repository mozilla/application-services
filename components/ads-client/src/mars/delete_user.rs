/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use serde::Serialize;

/// Body of `DELETE /delete_user`: asks MARS to forget everything it holds
/// for the given context id.
#[derive(Debug, Serialize)]
pub struct DeleteUserRequest<'a> {
    pub context_id: &'a str,
}

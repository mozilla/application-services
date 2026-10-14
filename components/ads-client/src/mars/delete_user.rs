/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use serde::Serialize;

/// Request body for the MARS `/v1/delete_user` endpoint.
#[derive(Debug, Serialize)]
pub struct DeleteUserRequest<'a> {
    pub context_id: &'a str,
}

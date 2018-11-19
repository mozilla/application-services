/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use errors::*;
use ClientInstance;
pub mod send_tab;

pub trait CommandsHandler {
    fn command_name() -> String
    where
        Self: Sized;
    fn as_any(&self) -> &dyn std::any::Any;
    /// Result is a tuple (Command value to register, local data to persist).
    fn init(&mut self, local_data: Option<&str>) -> Result<(String, String)>;
    fn handle_command(
        &mut self,
        local_data: &str,
        sender: Option<&ClientInstance>,
        payload: serde_json::Value,
    ) -> Result<()>;
}

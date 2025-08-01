/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub trait MapError {
    type Ok;

    fn map_to_viaduct_error(self) -> Result<Self::Ok, viaduct::Error>;
}

impl<T, E: ToString> MapError for Result<T, E> {
    type Ok = T;

    fn map_to_viaduct_error(self) -> Result<T, viaduct::Error> {
        self.map_err(|e| viaduct::Error::BackendError(e.to_string()))
    }
}

pub fn backend_error<T>(msg: impl Into<String>) -> Result<T, viaduct::Error> {
    Err(viaduct::Error::BackendError(msg.into()))
}

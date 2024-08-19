/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use crate::{error::Error, SearchApiResult};
use error_support::handle_error;

pub struct SearchEngineSelector();

impl SearchEngineSelector {
    #[handle_error(Error)]
    pub fn filter_engine_configuration(&self) -> SearchApiResult<()> {
        Err(Error::NotImplemented)
    }
}

#[cfg(test)]
mod tests {
    use super::SearchEngineSelector;

    #[test]
    fn test_filter_engine_config_throws() {
        let selector = SearchEngineSelector();

        let result = selector.filter_engine_configuration();

        assert!(result.is_err());
    }
}

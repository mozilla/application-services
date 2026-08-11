/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod builder;
mod clock;
mod connection_initializer;
mod outcome;
mod store;

use std::collections::HashMap;
use std::path::Path;

use self::builder::ImpressionLogBuilder;
use self::store::ImpressionLogStore;

pub use self::builder::ImpressionLogBuilderError;
pub use self::outcome::ImpressionLogOutcome;

#[derive(Clone, Copy, Debug, Default)]
pub enum ImpressionCappingPolicy {
    #[default]
    TelemetryOnly,
    ImpressionCapEnforced,
}

pub struct ImpressionLog {
    store: ImpressionLogStore,
}

impl ImpressionLog {
    pub fn builder<P: AsRef<Path>>(db_path: P) -> ImpressionLogBuilder {
        ImpressionLogBuilder::new(db_path.as_ref())
    }

    pub fn record_impression(&self, cap_key: &str) -> Result<(), rusqlite::Error> {
        self.store.record_impression(cap_key)?;
        Ok(())
    }

    pub fn count_impressions(
        &self,
        cap_keys: impl IntoIterator<Item = impl ToString>,
    ) -> Result<HashMap<String, u32>, rusqlite::Error> {
        let counts = self.store.count_impressions(cap_keys)?;
        Ok(counts)
    }

    pub fn retain_impressions(
        &self,
        cap_keys: impl IntoIterator<Item = impl ToString>,
    ) -> Result<(), rusqlite::Error> {
        self.store.retain_impressions(cap_keys)?;
        Ok(())
    }
}

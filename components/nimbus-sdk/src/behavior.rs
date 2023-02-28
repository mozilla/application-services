/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
#![allow(dead_code)]

use nimbus_core::behavior::{CoreEventStore, EventStore, MultiIntervalCounter};
use crate::error::{NimbusError, Result};
use crate::persistence::{Database, StoreId};
use std::collections::HashMap;

pub type SDKEventStore = Box<dyn DBBackedEventStore + Send>;

pub trait DBBackedEventStore: CoreEventStore {
    fn read_from_db(&mut self, db: &Database) -> Result<()>;

    fn persist_data(&self, db: &Database) -> Result<()>;

    fn clear_db(&mut self, db: &Database) -> Result<()>;
}

impl TryFrom<&Database> for EventStore {
    type Error = NimbusError;

    fn try_from(db: &Database) -> Result<Self, NimbusError> {
        let reader = db.read()?;
        let events = db
            .get_store(StoreId::EventCounts)
            .collect_all::<(String, MultiIntervalCounter), _>(&reader)?;
        Ok(EventStore::from(events))
    }
}

impl DBBackedEventStore for EventStore {
    fn read_from_db(&mut self, db: &Database) -> Result<()> {
        let reader = db.read()?;

        self.events = HashMap::from_iter(
            db.get_store(StoreId::EventCounts)
                .collect_all::<(String, MultiIntervalCounter), _>(&reader)?
                .into_iter(),
        );

        Ok(())
    }

    fn persist_data(&self, db: &Database) -> Result<()> {
        let mut writer = db.write()?;
        self.events.iter().try_for_each(|(key, value)| {
            db.get_store(StoreId::EventCounts)
                .put(&mut writer, key, &(key.clone(), value.clone()))
        })?;
        writer.commit()?;
        Ok(())
    }

    fn clear_db(&mut self, db: &Database) -> Result<()> {
        self.events = HashMap::<String, MultiIntervalCounter>::new();
        self.persist_data(db)?;
        Ok(())
    }
}

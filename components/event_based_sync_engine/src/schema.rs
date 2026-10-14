/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Schema evolution: write every version (backward compat) and migrate-on-read
//! (forward compat). The schema number belongs to the caller's domain, not the engine.
//!
//! * **Write**: authoring an event serializes the payload in every schema
//!   from the current version down to the floor (schema 3 writes `{3, 2, 1}`).
//!   Older clients read their own representation. They can't migrate a newer
//!   payload down themselves: they don't have the code for schemas after theirs.
//!   Only the writer knows both, so it does the down-migration.
//! * **Migrate-on-read**: reading an event that carries only an older schema
//!   applies the `n -> n+1` migrations in memory. The server record is untouched.
//!
//! Migrations must be pure, deterministic, total, and identical across clients.

use crate::error::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

/// The caller-domain payload schema version. Belongs to the reducer.
pub type SchemaVersion = u32;

/// A migration between two adjacent schema versions. Receives the event type
/// and the source-schema payload, returns the target-schema payload.
///
/// Must be pure, deterministic and total, or clients diverge.
pub type MigrateFn = Box<dyn Fn(&str, serde_json::Value) -> serde_json::Value + Send + Sync>;

/// The multi-schema payload on the wire, e.g. `{ "1": {..}, "2": {..} }`: keys
/// are schema versions, values the payload in that schema.
///
/// Keyed by `u32` for numeric ordering but serialized with stringified keys.
/// serde is hand-written because a `BTreeMap<u32, _>` fails to deserialize
/// inside an internally-tagged enum (buffered keys replay as strings); we parse
/// the key ourselves to avoid that.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Body {
    reps: BTreeMap<SchemaVersion, serde_json::Value>,
}

impl Serialize for Body {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.reps.len()))?;
        for (version, payload) in &self.reps {
            map.serialize_entry(&version.to_string(), payload)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Body {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Body;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map of stringified schema version to payload")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Body, M::Error> {
                let mut reps = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, serde_json::Value>()? {
                    let version = key
                        .parse::<SchemaVersion>()
                        .map_err(serde::de::Error::custom)?;
                    reps.insert(version, value);
                }
                Ok(Body { reps })
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

impl Body {
    pub fn new() -> Self {
        Self::default()
    }

    /// A body carrying a single schema representation. Mostly for tests; real
    /// authoring goes through [`SchemaRegistry::write`].
    pub fn single(version: SchemaVersion, payload: serde_json::Value) -> Self {
        let mut reps = BTreeMap::new();
        reps.insert(version, payload);
        Self { reps }
    }

    pub fn insert(&mut self, version: SchemaVersion, payload: serde_json::Value) {
        self.reps.insert(version, payload);
    }

    pub fn get(&self, version: SchemaVersion) -> Option<&serde_json::Value> {
        self.reps.get(&version)
    }

    /// The schema versions present, ascending.
    pub fn versions(&self) -> impl Iterator<Item = SchemaVersion> + '_ {
        self.reps.keys().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.reps.is_empty()
    }
}

/// Registry of schema migrations and the current/floor versions.
///
/// `up[n]` migrates `n-1 -> n` (used by migrate-on-read); `down[n]` migrates
/// `n -> n-1` (used by [`SchemaRegistry::write`]). Registering a migration to version `n`
/// implies the current schema is at least `n`.
#[derive(Default)]
pub struct SchemaRegistry {
    current: SchemaVersion,
    floor: SchemaVersion,
    up: BTreeMap<SchemaVersion, MigrateFn>,
    down: BTreeMap<SchemaVersion, MigrateFn>,
}

impl SchemaRegistry {
    /// A registry whose current schema and floor are both `floor`. Register
    /// migrations to raise the current version.
    pub fn new(floor: SchemaVersion) -> Self {
        Self {
            current: floor,
            floor,
            up: BTreeMap::new(),
            down: BTreeMap::new(),
        }
    }

    /// Register the migration pair that introduces schema `version`, raising the
    /// current schema to at least `version`.
    ///
    /// * `up` migrates `version - 1 -> version` (migrate-on-read)
    /// * `down` migrates `version -> version - 1` (write)
    pub fn register_migration(&mut self, version: SchemaVersion, up: MigrateFn, down: MigrateFn) {
        debug_assert!(version > self.floor, "cannot migrate below the floor");
        self.up.insert(version, up);
        self.down.insert(version, down);
        self.current = self.current.max(version);
    }

    pub fn current(&self) -> SchemaVersion {
        self.current
    }

    pub fn floor(&self) -> SchemaVersion {
        self.floor
    }

    /// Given a payload already in the current schema, render every
    /// representation from `current` down to `floor`.
    pub fn write(&self, event_type: &str, current_payload: serde_json::Value) -> Result<Body> {
        let mut body = Body::new();
        body.insert(self.current, current_payload.clone());

        let mut payload = current_payload;
        let mut version = self.current;
        while version > self.floor {
            let migrate = self.down.get(&version).ok_or(Error::MissingMigration {
                from: version,
                to: version - 1,
            })?;
            payload = migrate(event_type, payload);
            version -= 1;
            body.insert(version, payload.clone());
        }
        Ok(body)
    }

    /// Migrate-on-read: resolve `body` to a single payload readable at `reader`.
    ///
    /// * If the body carries `reader`, use it directly.
    /// * Otherwise migrate up the highest representation below `reader`.
    /// * If every representation is above `reader`, this client is below the
    ///   event's floor, so return [`Error::NoReadableSchema`].
    pub fn read(
        &self,
        event_type: &str,
        body: &Body,
        reader: SchemaVersion,
    ) -> Result<serde_json::Value> {
        if let Some(exact) = body.get(reader) {
            return Ok(exact.clone());
        }

        let start = body
            .versions()
            .filter(|v| *v < reader)
            .max()
            .ok_or_else(|| Error::NoReadableSchema {
                reader,
                available: body.versions().collect(),
            })?;

        let mut payload = body.get(start).expect("version came from the map").clone();
        let mut version = start;
        while version < reader {
            let migrate = self.up.get(&(version + 1)).ok_or(Error::MissingMigration {
                from: version,
                to: version + 1,
            })?;
            payload = migrate(event_type, payload);
            version += 1;
        }
        Ok(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A registry with a 1 <-> 2 migration: schema 2 renames `n` to `name`.
    fn registry_v2() -> SchemaRegistry {
        let mut reg = SchemaRegistry::new(1);
        reg.register_migration(
            2,
            // up: rename `n` to `name`.
            Box::new(|_ty, mut v| {
                let n = v.get_mut("n").map(|x| x.take()).unwrap_or(json!(""));
                json!({ "name": n })
            }),
            // down: rename `name` to `n`.
            Box::new(|_ty, mut v| {
                let name = v.get_mut("name").map(|x| x.take()).unwrap_or(json!(""));
                json!({ "n": name })
            }),
        );
        reg
    }

    #[test]
    fn write_renders_down_to_floor() {
        let reg = registry_v2();
        let body = reg.write("modify-name", json!({ "name": "foo" })).unwrap();
        assert_eq!(body.get(2), Some(&json!({ "name": "foo" })));
        assert_eq!(body.get(1), Some(&json!({ "n": "foo" })));
    }

    #[test]
    fn read_exact_version() {
        let reg = registry_v2();
        let body = reg.write("modify-name", json!({ "name": "foo" })).unwrap();
        assert_eq!(
            reg.read("modify-name", &body, 2).unwrap(),
            json!({ "name": "foo" })
        );
    }

    #[test]
    fn migrate_on_read_upgrades_old_event() {
        let reg = registry_v2();
        // A schema-1-only event authored before schema 2 existed.
        let body = Body::single(1, json!({ "n": "foo" }));
        let resolved = reg.read("modify-name", &body, 2).unwrap();
        assert_eq!(resolved, json!({ "name": "foo" }));
    }

    #[test]
    fn old_client_reads_its_own_representation() {
        let reg = registry_v2();
        let body = reg.write("modify-name", json!({ "name": "foo" })).unwrap();
        // A schema-1 client reads the schema-1 representation.
        assert_eq!(
            reg.read("modify-name", &body, 1).unwrap(),
            json!({ "n": "foo" })
        );
    }

    #[test]
    fn below_floor_event_is_unreadable() {
        let reg = registry_v2();
        // Only a schema-3 representation, but this client is v2.
        let body = Body::single(3, json!({ "whatever": true }));
        let err = reg.read("modify-name", &body, 2).unwrap_err();
        assert!(matches!(err, Error::NoReadableSchema { reader: 2, .. }));
    }

    #[test]
    fn wire_format_uses_stringified_versions() {
        let body = Body::single(1, json!({ "n": "foo" }));
        let wire = serde_json::to_string(&body).unwrap();
        assert_eq!(wire, r#"{"1":{"n":"foo"}}"#);
        let round: Body = serde_json::from_str(&wire).unwrap();
        assert_eq!(round, body);
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/// GeoNames support. GeoNames is an open-source geographical database of place
/// names worldwide, including cities, regions, and countries [1]. Notably it's
/// used by MaxMind's databases [2]. We use GeoNames to detect city and region
/// names and to map cities to regions. Specifically we use the data at [3];
/// also see [3] for documentation.
///
/// [1]: https://www.geonames.org/
/// [2]: https://www.maxmind.com/en/geoip-databases
/// [3]: https://download.geonames.org/export/dump/
use rusqlite::{named_params, Connection};
use serde::Deserialize;
use sql_support::ConnExt;
use std::{
    borrow::Cow,
    hash::{Hash, Hasher},
};
use unicase::UniCase;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use crate::{
    db::SuggestDao,
    error::RusqliteResultExt,
    metrics::MetricsContext,
    rs::{Client, Record, SuggestRecordId},
    store::SuggestStoreInner,
    Result,
};

/// The type of a geoname.
#[derive(Clone, Debug, Eq, Hash, PartialEq, uniffi::Enum)]
pub enum GeonameType {
    Country,
    /// A state, province, prefecture, district, borough, etc.
    Admin1,
    Admin2,
    Admin3,
    Admin4,
    AdminOther,
    /// A city, town, village, populated place, etc.
    City,
    Other,
}

pub type GeonameId = i64;

/// A single geographic place.
///
/// This corresponds to a single row in the main "geoname" table described in
/// the GeoNames documentation [1]. We exclude fields we don't need.
///
/// [1]: https://download.geonames.org/export/dump/readme.txt
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct Geoname {
    /// The `geonameid` straight from the geoname table.
    pub geoname_id: GeonameId,
    /// The geoname type. This is derived from `feature_class` and
    /// `feature_code` as a more convenient representation of the type.
    pub geoname_type: GeonameType,
    /// This is pretty much the place's canonical name. Usually there will be a
    /// row in the alternates table with the same name, but not always. When
    /// there is such a row, it doesn't always have `is_preferred_name` set, and
    /// in fact fact there may be another row with a different name with
    /// `is_preferred_name` set.
    pub name: String,
    /// ISO-3166 two-letter uppercase country code, e.g., "US".
    pub country_code: String,
    /// Primary geoname category. Examples:
    ///
    /// "PCLI" - Independent political entity: country
    /// "A" - Administrative division: state, province, borough, district, etc.
    /// "P" - Populated place: city, village, etc.
    pub feature_class: String,
    /// Secondary geoname category, depends on `feature_class`. Examples:
    ///
    /// "ADM1" - Administrative division 1
    /// "PPL" - Populated place like a city
    pub feature_code: String,
    /// Administrative division 1.
    pub admin1_code: Option<String>,
    /// Administrative division 2.
    pub admin2_code: Option<String>,
    /// Administrative division 3.
    pub admin3_code: Option<String>,
    /// Administrative division 4.
    pub admin4_code: Option<String>,
    /// Population size.
    pub population: u64,
    /// Latitude in decimal degrees (as a string).
    pub latitude: String,
    /// Longitude in decimal degrees (as a string).
    pub longitude: String,
}

impl Geoname {
    /// Whether `self` and `other` are related. For example, if one is a city
    /// and the other is an administrative division, this will return `true` if
    /// the city is located in the division.
    pub fn is_related_to(&self, other: &Self) -> bool {
        let self_level = usize::from(self.admin_level());
        let other_level = usize::from(other.admin_level());

        let self_admins = self.admin_array();
        let other_admins = other.admin_array();

        // Each admin level needs to be the same in `self` and `other` up to the
        // minimum level of `self` and `other`.
        for (level, (self_admin, other_admin)) in
            std::iter::zip(self_admins.iter(), other_admins.iter()).enumerate()
        {
            if self_level < level || other_level < level {
                break;
            }
            if self_admin != other_admin {
                return false;
            }
        }

        // At this point, admin levels are the same up to the minimum level. If
        // the types of `self` and `other` aren't the same, then one is an admin
        // division of the other. If they are the same type, then they need to
        // be the same geoname.
        self.geoname_type != other.geoname_type || self.geoname_id == other.geoname_id
    }

    fn admin_array(&self) -> [Option<&str>; 5] {
        [
            Some(&self.country_code),
            self.admin1_code.as_deref(),
            self.admin2_code.as_deref(),
            self.admin3_code.as_deref(),
            self.admin4_code.as_deref(),
        ]
    }

    fn admin_level(&self) -> u8 {
        match self.geoname_type {
            GeonameType::Country => 0,
            GeonameType::Admin1 => 1,
            GeonameType::Admin2 => 2,
            GeonameType::Admin3 => 3,
            _ => 4,
        }
    }
}

impl Hash for Geoname {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.geoname_id.hash(state)
    }
}

/// A fetched geoname with info on how it was matched.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct GeonameMatch {
    /// The geoname that was matched.
    pub geoname: Geoname,
    /// The type of name that was matched.
    pub match_type: GeonameMatchType,
    /// Whether the name was matched by prefix.
    pub prefix: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum GeonameMatchType {
    Abbreviation,
    AirportCode,
    /// This includes any names that aren't abbreviations or airport codes.
    Name,
}

impl GeonameMatchType {
    pub fn is_abbreviation(&self) -> bool {
        matches!(self, GeonameMatchType::Abbreviation)
    }

    pub fn is_name(&self) -> bool {
        matches!(self, GeonameMatchType::Name)
    }
}

/// This data is used to service every query handled by the weather provider and
/// potentially other providers, so we cache it from the DB.
#[derive(Debug, Default)]
pub struct GeonameCache {
    /// Max length of all geoname names.
    pub max_name_length: usize,
    /// Max word count across all geoname names.
    pub max_name_word_count: usize,
}

/// See `Geoname` for documentation.
#[derive(Clone, Debug, Deserialize)]
struct DownloadedGeoname {
    id: GeonameId,
    name: String,
    ascii_name: Option<String>,
    feature_class: String,
    feature_code: String,
    country: String,
    admin1: Option<String>,
    admin2: Option<String>,
    admin3: Option<String>,
    admin4: Option<String>,
    population: Option<u64>,
    latitude: Option<String>,
    longitude: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct DownloadedGeonamesAlternatesAttachment {
    /// The language of the names in this attachment as a lowercase ISO 639
    /// code: "en", "de", "fr", etc. Can also be a geonames pseduo-language like
    /// "abbr" for abbreviations and "iata" for airport codes.
    language: String,
    /// Tuples of geoname IDs and their localized names.
    names_by_geoname_id: Vec<(GeonameId, Vec<String>)>,
}

/// Compares two strings ignoring case, Unicode combining marks, and some
/// punctuation. Intended to be used as a Sqlite collating sequence for
/// comparing geoname names.
pub fn geonames_collate(a: &str, b: &str) -> std::cmp::Ordering {
    UniCase::new(collate_remove_chars(a)).cmp(&UniCase::new(collate_remove_chars(b)))
}

fn collate_remove_chars(s: &str) -> Cow<'_, str> {
    let borrowable = !s
        .nfkd()
        .any(|c| is_combining_mark(c) || matches!(c, '.' | ',' | '-'));

    if borrowable {
        Cow::from(s)
    } else {
        s.nfkd()
            .filter_map(|c| {
                if is_combining_mark(c) {
                    // Remove Unicode combining marks:
                    // "Que\u{0301}bec" => "Quebec"
                    None
                } else {
                    match c {
                        // Remove '.' and ',':
                        // "St. Louis, U.S.A." => "St Louis USA"
                        '.' | ',' => None,
                        // Replace '-' with space:
                        // "Carmel-by-the-Sea" => "Carmel by the Sea"
                        '-' => Some(' '),
                        _ => Some(c),
                    }
                }
            })
            .collect::<_>()
    }
}

impl SuggestDao<'_> {
    /// Fetches geonames that have at least one name matching the `query`
    /// string.
    ///
    /// `match_name_prefix` determines whether prefix matching is performed on
    /// names that aren't abbreviations and airport codes. When `true`, names
    /// that start with `query` will match. When false, names that equal `query`
    /// will match. Prefix matching is never performed on abbreviations and
    /// airport codes because we don't currently have a use case for that.
    ///
    /// `filter` restricts returned geonames to those that are related to the
    /// ones in the filter. Cities can be restricted to administrative divisions
    /// by including the divisions in `filter` and vice versa. This is
    /// especially useful since place names are not unique. `filter` is
    /// conjunctive: All geonames in `filter` must be related to a geoname in
    /// order for it to be filtered in.
    ///
    /// The returned matches will include all matching types for a geoname, one
    /// match per type per geoname. For example, if the query matches both a
    /// geoname's name and abbreviation, two matches for that geoname will be
    /// returned: one with a `match_type` of `GeonameMatchType::Name` and one
    /// with a `match_type` of `GeonameMatchType::Abbreviation`. `prefix` is set
    /// according to whether the query matched a prefix of the given type.
    pub fn fetch_geonames(
        &self,
        query: &str,
        match_name_prefix: bool,
        filter: Option<Vec<&Geoname>>,
    ) -> Result<Vec<GeonameMatch>> {
        let candidate_name = query;
        Ok(self
            .conn
            .query_rows_and_then_cached(
                r#"
                SELECT
                    g.id,
                    g.name,
                    g.feature_class,
                    g.feature_code,
                    g.country_code,
                    g.admin1_code,
                    g.admin2_code,
                    g.admin3_code,
                    g.admin4_code,
                    g.population,
                    g.latitude,
                    g.longitude,
                    a.name != :name AS prefix,
                    (SELECT CASE
                         -- abbreviation
                         WHEN a.language = 'abbr' THEN 1
                         -- airport code
                         WHEN a.language IN ('iata', 'icao', 'faac') THEN 2
                         -- name
                         ELSE 3
                         END
                    ) AS match_type
                FROM
                    geonames g
                JOIN
                    geonames_alternates a ON g.id = a.geoname_id
                WHERE
                    a.name = :name
                    OR (
                        :prefix
                        AND match_type = 3
                        AND (a.name BETWEEN :name AND :name || X'FFFF')
                    )
                GROUP BY
                    g.id, match_type
                ORDER BY
                    g.feature_class = 'P' DESC, g.population DESC, g.id ASC, a.language ASC
                "#,
                named_params! {
                    ":name": candidate_name,
                    ":prefix": match_name_prefix,
                },
                |row| -> Result<Option<GeonameMatch>> {
                    let feature_class: String = row.get("feature_class")?;
                    let feature_code: String = row.get("feature_code")?;
                    let geoname_type = match feature_class.as_str() {
                        "A" => {
                            if feature_code.starts_with("P") {
                                GeonameType::Country
                            } else {
                                match feature_code.as_str() {
                                    "ADM1" => GeonameType::Admin1,
                                    "ADM2" => GeonameType::Admin2,
                                    "ADM3" => GeonameType::Admin3,
                                    "ADM4" => GeonameType::Admin4,
                                    _ => GeonameType::AdminOther,
                                }
                            }
                        }
                        "P" => GeonameType::City,
                        _ => GeonameType::Other,
                    };
                    let g_match = GeonameMatch {
                        geoname: Geoname {
                            geoname_id: row.get("id")?,
                            geoname_type,
                            name: row.get("name")?,
                            feature_class,
                            feature_code,
                            country_code: row.get("country_code")?,
                            admin1_code: row.get("admin1_code")?,
                            admin2_code: row.get("admin2_code")?,
                            admin3_code: row.get("admin3_code")?,
                            admin4_code: row.get("admin4_code")?,
                            population: row
                                .get::<_, Option<u64>>("population")?
                                .unwrap_or_default(),
                            latitude: row
                                .get::<_, Option<String>>("latitude")?
                                .unwrap_or_default(),
                            longitude: row
                                .get::<_, Option<String>>("longitude")?
                                .unwrap_or_default(),
                        },
                        prefix: row.get("prefix")?,
                        match_type: match row.get::<_, i32>("match_type")? {
                            1 => GeonameMatchType::Abbreviation,
                            2 => GeonameMatchType::AirportCode,
                            _ => GeonameMatchType::Name,
                        },
                    };
                    if let Some(geonames) = &filter {
                        if geonames.iter().all(|g| g.is_related_to(&g_match.geoname)) {
                            Ok(Some(g_match))
                        } else {
                            Ok(None)
                        }
                    } else {
                        Ok(Some(g_match))
                    }
                },
            )?
            .into_iter()
            .flatten()
            .collect())
    }

    /// Inserts GeoNames data into the database.
    fn insert_geonames(
        &mut self,
        record_id: &SuggestRecordId,
        geonames: &[DownloadedGeoname],
    ) -> Result<()> {
        self.scope.err_if_interrupted()?;

        let mut geoname_insert = GeonameInsertStatement::new(self.conn)?;
        for geoname in geonames {
            geoname_insert.execute(record_id, geoname)?;
        }

        // Add alternates for each geoname's primary name (`geoname.name`) and
        // ASCII name. `language` is set to null for these alternates.
        self.insert_geonames_alternates_from_iter(
            record_id,
            None, // language
            geonames.iter().flat_map(|g| {
                [
                    Some((g.id, g.name.as_str())),
                    g.ascii_name.as_deref().map(|ascii_name| (g.id, ascii_name)),
                ]
                .into_iter()
                .flatten()
            }),
        )?;

        Ok(())
    }

    /// Inserts GeoNames alternates data into the database.
    fn insert_geonames_alternates(
        &mut self,
        record_id: &SuggestRecordId,
        attachments: &[DownloadedGeonamesAlternatesAttachment],
    ) -> Result<()> {
        for attach in attachments {
            self.insert_geonames_alternates_from_iter(
                record_id,
                Some(&attach.language),
                attach
                    .names_by_geoname_id
                    .iter()
                    .flat_map(|(geoname_id, names)| {
                        names.iter().map(|name| (*geoname_id, name.as_str()))
                    }),
            )?;
        }
        Ok(())
    }

    fn insert_geonames_alternates_from_iter<'a, I>(
        &mut self,
        record_id: &SuggestRecordId,
        language: Option<&str>,
        iter: I,
    ) -> Result<()>
    where
        I: Iterator<Item = (GeonameId, &'a str)>,
    {
        self.scope.err_if_interrupted()?;
        let mut alt_insert = GeonameAlternateInsertStatement::new(self.conn)?;
        let mut metrics_insert = GeonameMetricsInsertStatement::new(self.conn)?;
        let mut max_len = 0;
        let mut max_word_count = 0;
        for (geoname_id, name) in iter {
            alt_insert.execute(record_id, geoname_id, language, name)?;
            max_len = std::cmp::max(max_len, name.len());
            max_word_count = std::cmp::max(max_word_count, name.split_whitespace().count());
        }

        // Update alternates metrics.
        metrics_insert.execute(record_id, max_len, max_word_count)?;

        // We just made some insertions that might invalidate the data in the
        // cache. Clear it so it's repopulated the next time it's accessed.
        self.geoname_cache.take();

        Ok(())
    }

    pub fn geoname_cache(&self) -> &GeonameCache {
        self.geoname_cache.get_or_init(|| {
            self.conn
                .query_row_and_then(
                    r#"
                    SELECT
                        max(max_name_length) AS len, max(max_name_word_count) AS word_count
                    FROM
                        geonames_metrics
                    "#,
                    [],
                    |row| -> Result<GeonameCache> {
                        Ok(GeonameCache {
                            max_name_length: row.get("len")?,
                            max_name_word_count: row.get("word_count")?,
                        })
                    },
                )
                .unwrap_or_default()
        })
    }
}

impl<S> SuggestStoreInner<S>
where
    S: Client,
{
    /// Inserts a GeoNames record into the database.
    pub fn process_geonames_record(
        &self,
        dao: &mut SuggestDao,
        record: &Record,
        context: &mut MetricsContext,
    ) -> Result<()> {
        self.download_attachment(dao, record, context, |dao, record_id, data| {
            dao.insert_geonames(record_id, data)
        })
    }

    /// Inserts a GeoNames record into the database.
    pub fn process_geonames_alternates_record(
        &self,
        dao: &mut SuggestDao,
        record: &Record,
        context: &mut MetricsContext,
    ) -> Result<()> {
        self.download_attachment(dao, record, context, |dao, record_id, data| {
            dao.insert_geonames_alternates(record_id, data)
        })
    }
}

struct GeonameInsertStatement<'conn>(rusqlite::Statement<'conn>);

impl<'conn> GeonameInsertStatement<'conn> {
    fn new(conn: &'conn Connection) -> Result<Self> {
        Ok(Self(conn.prepare(
            "INSERT OR REPLACE INTO geonames(
                 id,
                 record_id,
                 name,
                 feature_class,
                 feature_code,
                 country_code,
                 admin1_code,
                 admin2_code,
                 admin3_code,
                 admin4_code,
                 population,
                 latitude,
                 longitude
             )
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ",
        )?))
    }

    fn execute(&mut self, record_id: &SuggestRecordId, g: &DownloadedGeoname) -> Result<()> {
        self.0
            .execute(rusqlite::params![
                &g.id,
                record_id.as_str(),
                &g.name,
                &g.feature_class,
                &g.feature_code,
                &g.country,
                &g.admin1,
                &g.admin2,
                &g.admin3,
                &g.admin4,
                &g.population,
                &g.latitude,
                &g.longitude,
            ])
            .with_context("geoname insert")?;
        Ok(())
    }
}

struct GeonameAlternateInsertStatement<'conn>(rusqlite::Statement<'conn>);

impl<'conn> GeonameAlternateInsertStatement<'conn> {
    fn new(conn: &'conn Connection) -> Result<Self> {
        Ok(Self(conn.prepare(
            "INSERT OR IGNORE INTO geonames_alternates(
                 record_id,
                 geoname_id,
                 language,
                 name
             )
             VALUES(?, ?, ?, ?)
             ",
        )?))
    }

    fn execute(
        &mut self,
        record_id: &SuggestRecordId,
        geoname_id: GeonameId,
        language: Option<&str>,
        name: &str,
    ) -> Result<()> {
        self.0
            .execute((record_id.as_str(), geoname_id, language, name))
            .with_context("geoname alternate insert")?;
        Ok(())
    }
}

struct GeonameMetricsInsertStatement<'conn>(rusqlite::Statement<'conn>);

impl<'conn> GeonameMetricsInsertStatement<'conn> {
    pub(crate) fn new(conn: &'conn Connection) -> Result<Self> {
        Ok(Self(conn.prepare(
            "INSERT OR REPLACE INTO geonames_metrics(
                 record_id,
                 max_name_length,
                 max_name_word_count
             )
             VALUES(?, ?, ?)
             ",
        )?))
    }

    pub(crate) fn execute(
        &mut self,
        record_id: &SuggestRecordId,
        max_len: usize,
        max_word_count: usize,
    ) -> Result<()> {
        self.0
            .execute((record_id.as_str(), max_len, max_word_count))
            .with_context("geoname metrics insert")?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        provider::SuggestionProvider,
        rs::{Collection, SuggestRecordType},
        store::tests::TestStore,
        testing::*,
        SuggestIngestionConstraints,
    };
    use itertools::Itertools;
    use serde_json::Value as JsonValue;

    pub(crate) const LONG_NAME: &str = "aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk lll mmm nnn ooo ppp qqq rrr sss ttt uuu vvv www x yyy zzz";

    pub(crate) fn geoname_mock_record(id: &str, json: JsonValue) -> MockRecord {
        MockRecord {
            collection: Collection::Other,
            record_type: SuggestRecordType::Geonames,
            id: id.to_string(),
            inline_data: None,
            attachment: Some(MockAttachment::Json(json)),
        }
    }

    pub(crate) fn geoname_alternates_mock_record(id: &str, json: JsonValue) -> MockRecord {
        MockRecord {
            collection: Collection::Other,
            record_type: SuggestRecordType::GeonamesAlternates,
            id: id.to_string(),
            inline_data: None,
            attachment: Some(MockAttachment::Json(json)),
        }
    }

    pub(crate) fn new_test_store() -> TestStore {
        TestStore::new(
            MockRemoteSettingsClient::default()
                .with_record(geoname_mock_record("geonames-0", geonames_data()))
                .with_record(geoname_alternates_mock_record(
                    "geonames-alternates-en",
                    geonames_alternates_data_en(),
                ))
                .with_record(geoname_alternates_mock_record(
                    "geonames-alternates-abbr",
                    geonames_alternates_data_abbr(),
                ))
                .with_record(geoname_alternates_mock_record(
                    "geonames-alternates-iata",
                    geonames_alternates_data_iata(),
                )),
        )
    }

    fn geonames_data() -> serde_json::Value {
        json!([
            // Waterloo, AL
            {
                "id": 4096497,
                "name": "Waterloo",
                "feature_class": "P",
                "feature_code": "PPL",
                "country": "US",
                "admin1": "AL",
                "admin2": "077",
                "population": 200,
                "latitude": "34.91814",
                "longitude": "-88.0642",
            },
            // AL
            {
                "id": 4829764,
                "name": "Alabama",
                "feature_class": "A",
                "feature_code": "ADM1",
                "country": "US",
                "admin1": "AL",
                "population": 4530315,
                "latitude": "32.75041",
                "longitude": "-86.75026",
            },
            // Waterloo, IA
            {
                "id": 4880889,
                "name": "Waterloo",
                "feature_class": "P",
                "feature_code": "PPLA2",
                "country": "US",
                "admin1": "IA",
                "admin2": "013",
                "admin3": "94597",
                "population": 68460,
                "latitude": "42.49276",
                "longitude": "-92.34296",
            },
            // IA
            {
                "id": 4862182,
                "name": "Iowa",
                "feature_class": "A",
                "feature_code": "ADM1",
                "country": "US",
                "admin1": "IA",
                "population": 2955010,
                "latitude": "42.00027",
                "longitude": "-93.50049",
            },
            // New York City
            {
                "id": 5128581,
                "name": "New York City",
                "feature_class": "P",
                "feature_code": "PPL",
                "country": "US",
                "admin1": "NY",
                "population": 8804190,
                "latitude": "40.71427",
                "longitude": "-74.00597",
            },
            // Rochester, NY
            {
                "id": 5134086,
                "name": "Rochester",
                "feature_class": "P",
                "feature_code": "PPLA2",
                "country": "US",
                "admin1": "NY",
                "admin2": "055",
                "admin3": "63000",
                "population": 209802,
                "latitude": "43.15478",
                "longitude": "-77.61556",
            },
            // NY state
            {
                "id": 5128638,
                "name": "New York",
                "feature_class": "A",
                "feature_code": "ADM1",
                "country": "US",
                "admin1": "NY",
                "population": 19274244,
                "latitude": "43.00035",
                "longitude": "-75.4999",
            },
            // Waco, TX: Has a surprising IATA airport code that's a
            // common English word and not a prefix of the city name
            {
                "id": 4739526,
                "name": "Waco",
                "feature_class": "P",
                "feature_code": "PPLA2",
                "country": "US",
                "admin1": "TX",
                "admin2": "309",
                "population": 132356,
                "latitude": "31.54933",
                "longitude": "-97.14667",
            },
            // TX
            {
                "id": 4736286,
                "name": "Texas",
                "feature_class": "A",
                "feature_code": "ADM1",
                "country": "US",
                "admin1": "TX",
                "population": 22875689,
                "latitude": "31.25044",
                "longitude": "-99.25061",
            },
            // Made-up city with a long name
            {
                "id": 999,
                "name": "Long Name",
                "feature_class": "P",
                "feature_code": "PPLA2",
                "country": "US",
                "admin1": "NY",
                "population": 2,
                "latitude": "38.06084",
                "longitude": "-97.92977",
            },
            // St. Louis (has '.' in name)
            {
                "id": 4407066,
                "name": "St. Louis",
                "feature_class": "P",
                "feature_code": "PPLA2",
                "country": "US",
                "admin1": "MO",
                "admin2": "510",
                "population": 315685,
                "latitude": "38.62727",
                "longitude": "-90.19789",
            },
            // Carmel-by-the-Sea (has '-' in name)
            {
                "id": 5334320,
                "name": "Carmel-by-the-Sea",
                "feature_class": "P",
                "feature_code": "PPL",
                "country": "US",
                "admin1": "CA",
                "admin2": "053",
                "population": 3897,
                "latitude": "36.55524",
                "longitude": "-121.92329",
            },
            // United States
            {
                "id": 6252001,
                "name": "United States",
                "feature_class": "A",
                "feature_code": "PCLI",
                "country": "US",
                "admin1": "00",
                "population": 327167434,
                "latitude": "39.76",
                "longitude": "-98.5",
            },
            // Canada
            {
                "id": 6251999,
                "name": "Canada",
                "feature_class": "A",
                "feature_code": "PCLI",
                "country": "CA",
                "admin1": "00",
                "population": 37058856,
                "latitude": "60.10867",
                "longitude": "-113.64258",
            },
            // ON
            {
                "id": 6093943,
                "name": "Ontario",
                "feature_class": "A",
                "feature_code": "ADM1",
                "country": "CA",
                "admin1": "08",
                "population": 12861940,
                "latitude": "49.25014",
                "longitude": "-84.49983",
            },
            // Waterloo, ON
            {
                "id": 6176823,
                "name": "Waterloo",
                "feature_class": "P",
                "feature_code": "PPL",
                "country": "CA",
                "admin1": "08",
                "admin2": "3530",
                "population": 104986,
                "latitude": "43.4668",
                "longitude": "-80.51639",
            },
            // UK
            {
                "id": 2635167,
                "name": "United Kingdom of Great Britain and Northern Ireland",
                "feature_class": "A",
                "feature_code": "PCLI",
                "country": "GB",
                "admin1": "00",
                "population": 66488991,
                "latitude": "54.75844",
                "longitude": "-2.69531",
            },
            // England
            {
                "id": 6269131,
                "name": "England",
                "feature_class": "A",
                "feature_code": "ADM1",
                "country": "GB",
                "admin1": "ENG",
                "population": 57106398,
                "latitude": "52.16045",
                "longitude": "-0.70312",
            },
            // Liverpool (metropolitan borough, admin2 for Liverpool city)
            {
                "id": 3333167,
                "name": "Liverpool",
                "feature_class": "A",
                "feature_code": "ADM2",
                "country": "GB",
                "admin1": "ENG",
                "admin2": "H8",
                "population": 484578,
                "latitude": "53.41667",
                "longitude": "-2.91667",
            },
            // Liverpool (city)
            {
                "id": 2644210,
                "name": "Liverpool",
                "feature_class": "P",
                "feature_code": "PPLA2",
                "country": "GB",
                "admin1": "ENG",
                "admin2": "H8",
                "population": 864122,
                "latitude": "53.41058",
                "longitude": "-2.97794",
            },
            // Gößnitz, DE (has non-basic-Latin chars and an `ascii_name`)
            {
                "id": 2918770,
                "name": "Gößnitz",
                "ascii_name": "Goessnitz",
                "feature_class": "P",
                "feature_code": "PPL",
                "country": "DE",
                "admin1": "15",
                "admin2": "00",
                "admin3": "16077",
                "admin4": "16077012",
                "population": 4104,
                "latitude": "50.88902",
                "longitude": "12.43292",
            },
        ])
    }

    fn geonames_alternates_data_en() -> serde_json::Value {
        json!({
            "language": "en",
            "names_by_geoname_id": [
                // United States
                [6252001, [
                    "United States",
                    "America",
                    "United States of America",
                    "USA",
                ]],
                // UK
                [2635167, [
                    "Great Britain",
                    "Britain",
                    "United Kingdom",
                    "UK",
                    "U.K.",
                    "United Kingdom of Great Britain and Northern Ireland",
                    "U.K",
                ]],
                // New York City
                [5128581, [
                    "New York",
                ]],
                // Made-up city with a long name
                [999, [LONG_NAME]],
            ],
        })
    }

    fn geonames_alternates_data_abbr() -> serde_json::Value {
        json!({
            "language": "abbr",
            "names_by_geoname_id": [
                // AL
                [4829764, ["AL"]],
                // IA
                [4862182, ["IA"]],
                // ON
                [6093943, [
                    "ON",
                    "Ont.",
                ]],
                // NY State
                [5128638, ["NY"]],
                // TX
                [4736286, ["TX"]],
                // New York City
                [5128581, [
                    "NYC",
                    "NY",
                ]],
                // United States
                [6252001, [
                    "U.S.",
                    "USA",
                    "U.S.A.",
                    "US",
                ]],
                // Liverpool (metropolitan borough, admin2 for Liverpool city)
                [3333167, ["LIV"]],
                // UK
                [2635167, [
                    "Great Britain",
                    "Britain",
                    "United Kingdom",
                    "UK",
                    "U.K.",
                    "United Kingdom of Great Britain and Northern Ireland",
                    "U.K",
                ]],
            ],
        })
    }

    fn geonames_alternates_data_iata() -> serde_json::Value {
        json!({
            "language": "iata",
            "names_by_geoname_id": [
                // Waco, TX
                [4739526, ["ACT"]],
                // Rochester, NY
                [5134086, ["ROC"]],
            ],
        })
    }

    pub(crate) fn waterloo_al() -> Geoname {
        Geoname {
            geoname_id: 4096497,
            geoname_type: GeonameType::City,
            name: "Waterloo".to_string(),
            feature_class: "P".to_string(),
            feature_code: "PPL".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("AL".to_string()),
            admin2_code: Some("077".to_string()),
            admin3_code: None,
            admin4_code: None,
            population: 200,
            latitude: "34.91814".to_string(),
            longitude: "-88.0642".to_string(),
        }
    }

    pub(crate) fn waterloo_ia() -> Geoname {
        Geoname {
            geoname_id: 4880889,
            geoname_type: GeonameType::City,
            name: "Waterloo".to_string(),
            feature_class: "P".to_string(),
            feature_code: "PPLA2".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("IA".to_string()),
            admin2_code: Some("013".to_string()),
            admin3_code: Some("94597".to_string()),
            admin4_code: None,
            population: 68460,
            latitude: "42.49276".to_string(),
            longitude: "-92.34296".to_string(),
        }
    }

    pub(crate) fn nyc() -> Geoname {
        Geoname {
            geoname_id: 5128581,
            geoname_type: GeonameType::City,
            name: "New York City".to_string(),
            feature_class: "P".to_string(),
            feature_code: "PPL".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("NY".to_string()),
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: 8804190,
            latitude: "40.71427".to_string(),
            longitude: "-74.00597".to_string(),
        }
    }

    pub(crate) fn rochester() -> Geoname {
        Geoname {
            geoname_id: 5134086,
            geoname_type: GeonameType::City,
            name: "Rochester".to_string(),
            feature_class: "P".to_string(),
            feature_code: "PPLA2".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("NY".to_string()),
            admin2_code: Some("055".to_string()),
            admin3_code: Some("63000".to_string()),
            admin4_code: None,
            population: 209802,
            latitude: "43.15478".to_string(),
            longitude: "-77.61556".to_string(),
        }
    }

    pub(crate) fn waco() -> Geoname {
        Geoname {
            geoname_id: 4739526,
            geoname_type: GeonameType::City,
            name: "Waco".to_string(),
            feature_class: "P".to_string(),
            feature_code: "PPLA2".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("TX".to_string()),
            admin2_code: Some("309".to_string()),
            admin3_code: None,
            admin4_code: None,
            population: 132356,
            latitude: "31.54933".to_string(),
            longitude: "-97.14667".to_string(),
        }
    }

    pub(crate) fn long_name_city() -> Geoname {
        Geoname {
            geoname_id: 999,
            geoname_type: GeonameType::City,
            name: "Long Name".to_string(),
            feature_class: "P".to_string(),
            feature_code: "PPLA2".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("NY".to_string()),
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: 2,
            latitude: "38.06084".to_string(),
            longitude: "-97.92977".to_string(),
        }
    }

    pub(crate) fn al() -> Geoname {
        Geoname {
            geoname_id: 4829764,
            geoname_type: GeonameType::Admin1,
            name: "Alabama".to_string(),
            feature_class: "A".to_string(),
            feature_code: "ADM1".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("AL".to_string()),
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: 4530315,
            latitude: "32.75041".to_string(),
            longitude: "-86.75026".to_string(),
        }
    }

    pub(crate) fn ia() -> Geoname {
        Geoname {
            geoname_id: 4862182,
            geoname_type: GeonameType::Admin1,
            name: "Iowa".to_string(),
            feature_class: "A".to_string(),
            feature_code: "ADM1".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("IA".to_string()),
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: 2955010,
            latitude: "42.00027".to_string(),
            longitude: "-93.50049".to_string(),
        }
    }

    pub(crate) fn ny_state() -> Geoname {
        Geoname {
            geoname_id: 5128638,
            geoname_type: GeonameType::Admin1,
            name: "New York".to_string(),
            feature_class: "A".to_string(),
            feature_code: "ADM1".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("NY".to_string()),
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: 19274244,
            latitude: "43.00035".to_string(),
            longitude: "-75.4999".to_string(),
        }
    }

    pub(crate) fn st_louis() -> Geoname {
        Geoname {
            geoname_id: 4407066,
            geoname_type: GeonameType::City,
            name: "St. Louis".to_string(),
            feature_class: "P".to_string(),
            feature_code: "PPLA2".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("MO".to_string()),
            admin2_code: Some("510".to_string()),
            admin3_code: None,
            admin4_code: None,
            population: 315685,
            latitude: "38.62727".to_string(),
            longitude: "-90.19789".to_string(),
        }
    }

    pub(crate) fn carmel() -> Geoname {
        Geoname {
            geoname_id: 5334320,
            geoname_type: GeonameType::City,
            name: "Carmel-by-the-Sea".to_string(),
            feature_class: "P".to_string(),
            feature_code: "PPL".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("CA".to_string()),
            admin2_code: Some("053".to_string()),
            admin3_code: None,
            admin4_code: None,
            population: 3897,
            latitude: "36.55524".to_string(),
            longitude: "-121.92329".to_string(),
        }
    }

    pub(crate) fn us() -> Geoname {
        Geoname {
            geoname_id: 6252001,
            geoname_type: GeonameType::Country,
            name: "United States".to_string(),
            feature_class: "A".to_string(),
            feature_code: "PCLI".to_string(),
            country_code: "US".to_string(),
            admin1_code: Some("00".to_string()),
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: 327167434,
            latitude: "39.76".to_string(),
            longitude: "-98.5".to_string(),
        }
    }

    pub(crate) fn canada() -> Geoname {
        Geoname {
            geoname_id: 6251999,
            geoname_type: GeonameType::Country,
            name: "Canada".to_string(),
            feature_class: "A".to_string(),
            feature_code: "PCLI".to_string(),
            country_code: "CA".to_string(),
            admin1_code: Some("00".to_string()),
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: 37058856,
            latitude: "60.10867".to_string(),
            longitude: "-113.64258".to_string(),
        }
    }

    pub(crate) fn on() -> Geoname {
        Geoname {
            geoname_id: 6093943,
            geoname_type: GeonameType::Admin1,
            name: "Ontario".to_string(),
            feature_class: "A".to_string(),
            feature_code: "ADM1".to_string(),
            country_code: "CA".to_string(),
            admin1_code: Some("08".to_string()),
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: 12861940,
            latitude: "49.25014".to_string(),
            longitude: "-84.49983".to_string(),
        }
    }

    pub(crate) fn waterloo_on() -> Geoname {
        Geoname {
            geoname_id: 6176823,
            geoname_type: GeonameType::City,
            name: "Waterloo".to_string(),
            feature_class: "P".to_string(),
            feature_code: "PPL".to_string(),
            country_code: "CA".to_string(),
            admin1_code: Some("08".to_string()),
            admin2_code: Some("3530".to_string()),
            admin3_code: None,
            admin4_code: None,
            population: 104986,
            latitude: "43.4668".to_string(),
            longitude: "-80.51639".to_string(),
        }
    }

    pub(crate) fn uk() -> Geoname {
        Geoname {
            geoname_id: 2635167,
            geoname_type: GeonameType::Country,
            name: "United Kingdom of Great Britain and Northern Ireland".to_string(),
            feature_class: "A".to_string(),
            feature_code: "PCLI".to_string(),
            country_code: "GB".to_string(),
            admin1_code: Some("00".to_string()),
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: 66488991,
            latitude: "54.75844".to_string(),
            longitude: "-2.69531".to_string(),
        }
    }

    pub(crate) fn england() -> Geoname {
        Geoname {
            geoname_id: 6269131,
            geoname_type: GeonameType::Admin1,
            name: "England".to_string(),
            feature_class: "A".to_string(),
            feature_code: "ADM1".to_string(),
            country_code: "GB".to_string(),
            admin1_code: Some("ENG".to_string()),
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: 57106398,
            latitude: "52.16045".to_string(),
            longitude: "-0.70312".to_string(),
        }
    }

    pub(crate) fn liverpool_metro() -> Geoname {
        Geoname {
            geoname_id: 3333167,
            geoname_type: GeonameType::Admin2,
            name: "Liverpool".to_string(),
            feature_class: "A".to_string(),
            feature_code: "ADM2".to_string(),
            country_code: "GB".to_string(),
            admin1_code: Some("ENG".to_string()),
            admin2_code: Some("H8".to_string()),
            admin3_code: None,
            admin4_code: None,
            population: 484578,
            latitude: "53.41667".to_string(),
            longitude: "-2.91667".to_string(),
        }
    }

    pub(crate) fn liverpool_city() -> Geoname {
        Geoname {
            geoname_id: 2644210,
            geoname_type: GeonameType::City,
            name: "Liverpool".to_string(),
            feature_class: "P".to_string(),
            feature_code: "PPLA2".to_string(),
            country_code: "GB".to_string(),
            admin1_code: Some("ENG".to_string()),
            admin2_code: Some("H8".to_string()),
            admin3_code: None,
            admin4_code: None,
            population: 864122,
            latitude: "53.41058".to_string(),
            longitude: "-2.97794".to_string(),
        }
    }

    pub(crate) fn goessnitz() -> Geoname {
        Geoname {
            geoname_id: 2918770,
            geoname_type: GeonameType::City,
            name: "Gößnitz".to_string(),
            feature_class: "P".to_string(),
            feature_code: "PPL".to_string(),
            country_code: "DE".to_string(),
            admin1_code: Some("15".to_string()),
            admin2_code: Some("00".to_string()),
            admin3_code: Some("16077".to_string()),
            admin4_code: Some("16077012".to_string()),
            population: 4104,
            latitude: "50.88902".to_string(),
            longitude: "12.43292".to_string(),
        }
    }

    #[test]
    fn is_related_to() -> anyhow::Result<()> {
        // The geonames in each vec should be pairwise related.
        let tests = [
            vec![waterloo_ia(), ia(), us()],
            vec![waterloo_al(), al(), us()],
            vec![waterloo_on(), on(), canada()],
            vec![liverpool_city(), liverpool_metro(), england(), uk()],
        ];
        for geonames in tests {
            for g in &geonames {
                // A geoname should always be related to itself.
                assert!(
                    g.is_related_to(g),
                    "g.is_related_to(g) should always be true: {:?}",
                    g
                );
            }
            for a_and_b in geonames.iter().permutations(2) {
                assert!(
                    a_and_b[0].is_related_to(a_and_b[1]),
                    "is_related_to: {:?}",
                    a_and_b
                );
            }
        }
        Ok(())
    }

    #[test]
    fn is_not_related_to() -> anyhow::Result<()> {
        // The geonames in each vec should not be pairwise related.
        let tests = [
            vec![waterloo_ia(), al()],
            vec![waterloo_ia(), on()],
            vec![waterloo_ia(), canada(), uk()],
            vec![waterloo_al(), ia()],
            vec![waterloo_al(), on()],
            vec![waterloo_al(), canada(), uk()],
            vec![waterloo_on(), al()],
            vec![waterloo_on(), ia()],
            vec![waterloo_on(), us(), uk()],
            vec![
                waterloo_ia(),
                waterloo_al(),
                waterloo_on(),
                liverpool_city(),
            ],
            vec![liverpool_city(), us(), canada()],
            vec![liverpool_metro(), us(), canada()],
            vec![england(), us(), canada()],
            vec![al(), ia(), on(), england()],
            vec![us(), canada(), uk()],
        ];
        for geonames in tests {
            for a_and_b in geonames.iter().permutations(2) {
                assert!(
                    !a_and_b[0].is_related_to(a_and_b[1]),
                    "!is_related_to: {:?}",
                    a_and_b
                );
            }
        }
        Ok(())
    }

    #[test]
    fn geonames_collate() -> anyhow::Result<()> {
        let tests = [
            ["AbC xYz", "ABC XYZ", "abc xyz"].as_slice(),
            &["Àęí", "Aei", "àęí", "aei"],
            &[
                // "Québec" with single 'é' char
                "Qu\u{00e9}bec",
                // "Québec" with ASCII 'e' followed by combining acute accent
                "Que\u{0301}bec",
                "Quebec",
                "quebec",
            ],
            &[
                "Gößnitz",
                "Gössnitz",
                "Goßnitz",
                "Gossnitz",
                "gößnitz",
                "gössnitz",
                "goßnitz",
                "gossnitz",
            ],
            &["St. Louis", "St... Louis", "St Louis"],
            &["U.S.A.", "US.A.", "U.SA.", "U.S.A", "USA.", "U.SA", "USA"],
            &["Carmel-by-the-Sea", "Carmel by the Sea"],
            &[".,-'()[]?<>", ".,-'()[]?<>"],
        ];
        for strs in tests {
            for a_and_b in strs.iter().permutations(2) {
                assert_eq!(
                    super::geonames_collate(a_and_b[0], a_and_b[1]),
                    std::cmp::Ordering::Equal,
                    "Comparing: {:?}",
                    a_and_b
                );
            }
        }
        Ok(())
    }

    #[test]
    fn geonames() -> anyhow::Result<()> {
        before_each();

        let store = new_test_store();

        // Ingest weather to also ingest geonames.
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        #[derive(Debug)]
        struct Test {
            query: &'static str,
            match_name_prefix: bool,
            filter: Option<Vec<Geoname>>,
            expected: Vec<GeonameMatch>,
        }

        let tests = [
            Test {
                query: "ia",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: ia(),
                    match_type: GeonameMatchType::Abbreviation,
                    prefix: false,
                }],
            },
            Test {
                query: "ia",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: ia(),
                    match_type: GeonameMatchType::Abbreviation,
                    prefix: false,
                }],
            },
            Test {
                query: "ia",
                match_name_prefix: false,
                filter: Some(vec![waterloo_ia(), waterloo_al()]),
                expected: vec![],
            },
            Test {
                query: "ia",
                match_name_prefix: false,
                filter: Some(vec![waterloo_ia()]),
                expected: vec![GeonameMatch {
                    geoname: ia(),
                    match_type: GeonameMatchType::Abbreviation,
                    prefix: false,
                }],
            },
            Test {
                query: "ia",
                match_name_prefix: false,
                filter: Some(vec![us()]),
                expected: vec![GeonameMatch {
                    geoname: ia(),
                    match_type: GeonameMatchType::Abbreviation,
                    prefix: false,
                }],
            },
            Test {
                query: "ia",
                match_name_prefix: false,
                filter: Some(vec![waterloo_al()]),
                expected: vec![],
            },
            Test {
                query: "ia",
                match_name_prefix: false,
                filter: Some(vec![canada()]),
                expected: vec![],
            },
            Test {
                query: "ia",
                match_name_prefix: false,
                filter: Some(vec![uk()]),
                expected: vec![],
            },
            Test {
                query: "iaxyz",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "iaxyz",
                match_name_prefix: true,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "iowa",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: ia(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "al",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: al(),
                    match_type: GeonameMatchType::Abbreviation,
                    prefix: false,
                }],
            },
            // "al" is both a name prefix and an abbreviation.
            Test {
                query: "al",
                match_name_prefix: true,
                filter: None,
                expected: vec![
                    GeonameMatch {
                        geoname: al(),
                        match_type: GeonameMatchType::Name,
                        prefix: true,
                    },
                    GeonameMatch {
                        geoname: al(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![ia()]),
                expected: vec![GeonameMatch {
                    geoname: waterloo_ia(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![al()]),
                expected: vec![GeonameMatch {
                    geoname: waterloo_al(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![ny_state()]),
                expected: vec![],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: None,
                // Matches should be returned by population descending.
                expected: vec![
                    GeonameMatch {
                        geoname: waterloo_on(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: waterloo_ia(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: waterloo_al(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "water",
                match_name_prefix: true,
                filter: None,
                expected: vec![
                    GeonameMatch {
                        geoname: waterloo_on(),
                        match_type: GeonameMatchType::Name,
                        prefix: true,
                    },
                    GeonameMatch {
                        geoname: waterloo_ia(),
                        match_type: GeonameMatchType::Name,
                        prefix: true,
                    },
                    GeonameMatch {
                        geoname: waterloo_al(),
                        match_type: GeonameMatchType::Name,
                        prefix: true,
                    },
                ],
            },
            Test {
                query: "water",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![us()]),
                expected: vec![
                    GeonameMatch {
                        geoname: waterloo_ia(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: waterloo_al(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![al(), us()]),
                expected: vec![GeonameMatch {
                    geoname: waterloo_al(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![us(), al()]),
                expected: vec![GeonameMatch {
                    geoname: waterloo_al(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![ia(), al()]),
                expected: vec![],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![canada()]),
                expected: vec![GeonameMatch {
                    geoname: waterloo_on(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![on()]),
                expected: vec![GeonameMatch {
                    geoname: waterloo_on(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![on(), canada()]),
                expected: vec![GeonameMatch {
                    geoname: waterloo_on(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![canada(), on()]),
                expected: vec![GeonameMatch {
                    geoname: waterloo_on(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![al(), canada()]),
                expected: vec![],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![on(), us()]),
                expected: vec![],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![waterloo_al()]),
                expected: vec![GeonameMatch {
                    geoname: waterloo_al(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "waterloo",
                match_name_prefix: false,
                filter: Some(vec![uk()]),
                expected: vec![],
            },
            Test {
                query: "waterlooxyz",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "waterlooxyz",
                match_name_prefix: true,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "waterloo xyz",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "waterloo xyz",
                match_name_prefix: true,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "ny",
                match_name_prefix: false,
                filter: None,
                // NYC should be first since cities are ordered before regions.
                expected: vec![
                    GeonameMatch {
                        geoname: nyc(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: ny_state(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "ny",
                match_name_prefix: false,
                filter: Some(vec![nyc()]),
                expected: vec![
                    GeonameMatch {
                        geoname: nyc(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: ny_state(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "ny",
                match_name_prefix: false,
                filter: Some(vec![ny_state()]),
                expected: vec![
                    GeonameMatch {
                        geoname: nyc(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: ny_state(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "nyc",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: nyc(),
                    match_type: GeonameMatchType::Abbreviation,
                    prefix: false,
                }],
            },
            Test {
                query: "NeW YoRk",
                match_name_prefix: false,
                filter: None,
                expected: vec![
                    GeonameMatch {
                        geoname: nyc(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: ny_state(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "NY",
                match_name_prefix: false,
                filter: None,
                expected: vec![
                    GeonameMatch {
                        geoname: nyc(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: ny_state(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "new",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "new",
                match_name_prefix: true,
                filter: None,
                expected: vec![
                    GeonameMatch {
                        geoname: nyc(),
                        match_type: GeonameMatchType::Name,
                        prefix: true,
                    },
                    GeonameMatch {
                        geoname: ny_state(),
                        match_type: GeonameMatchType::Name,
                        prefix: true,
                    },
                ],
            },
            Test {
                query: "new york foo",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "new york foo",
                match_name_prefix: true,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "new foo",
                match_name_prefix: true,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "foo new york",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "foo new york",
                match_name_prefix: true,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "foo new",
                match_name_prefix: true,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "roc",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: rochester(),
                    match_type: GeonameMatchType::AirportCode,
                    prefix: false,
                }],
            },
            // "roc" is both a name prefix and an airport code.
            Test {
                query: "roc",
                match_name_prefix: true,
                filter: None,
                expected: vec![
                    GeonameMatch {
                        geoname: rochester(),
                        match_type: GeonameMatchType::Name,
                        prefix: true,
                    },
                    GeonameMatch {
                        geoname: rochester(),
                        match_type: GeonameMatchType::AirportCode,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "long name",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: long_name_city(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: LONG_NAME,
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: long_name_city(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "ac",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "ac",
                match_name_prefix: true,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "act",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: waco(),
                    match_type: GeonameMatchType::AirportCode,
                    prefix: false,
                }],
            },
            Test {
                query: "act",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: waco(),
                    match_type: GeonameMatchType::AirportCode,
                    prefix: false,
                }],
            },
            Test {
                query: "us",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: us(),
                    match_type: GeonameMatchType::Abbreviation,
                    prefix: false,
                }],
            },
            Test {
                query: "us",
                match_name_prefix: false,
                filter: Some(vec![waterloo_ia()]),
                expected: vec![GeonameMatch {
                    geoname: us(),
                    match_type: GeonameMatchType::Abbreviation,
                    prefix: false,
                }],
            },
            Test {
                query: "us",
                match_name_prefix: false,
                filter: Some(vec![ia()]),
                expected: vec![GeonameMatch {
                    geoname: us(),
                    match_type: GeonameMatchType::Abbreviation,
                    prefix: false,
                }],
            },
            Test {
                query: "canada",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: canada(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "canada",
                match_name_prefix: false,
                filter: Some(vec![on()]),
                expected: vec![GeonameMatch {
                    geoname: canada(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "canada",
                match_name_prefix: false,
                filter: Some(vec![waterloo_on(), on()]),
                expected: vec![GeonameMatch {
                    geoname: canada(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "uk",
                match_name_prefix: false,
                filter: None,
                expected: vec![
                    // "UK" is listed as both an 'en' alternate and 'abbr'
                    // alternate. The abbreviation should be first since 'abbr'
                    // is ordered before 'en'.
                    GeonameMatch {
                        geoname: uk(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: uk(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "st. louis",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: st_louis(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "st louis",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: st_louis(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "st.",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: st_louis(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "st. l",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: st_louis(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "st l",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: st_louis(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "st.",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "st l",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "carmel-by-the-sea",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: carmel(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "carmel by the sea",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: carmel(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "carmel-",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: carmel(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "carmel-b",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: carmel(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "carmel b",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: carmel(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "carmel-",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "carmel-b",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "carmel b",
                match_name_prefix: false,
                filter: None,
                expected: vec![],
            },
            Test {
                query: "liverpool",
                match_name_prefix: false,
                filter: None,
                expected: vec![
                    GeonameMatch {
                        geoname: liverpool_city(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: liverpool_metro(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "liverpool",
                match_name_prefix: false,
                filter: Some(vec![liverpool_metro()]),
                expected: vec![
                    GeonameMatch {
                        geoname: liverpool_city(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: liverpool_metro(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "liverpool",
                match_name_prefix: false,
                filter: Some(vec![england()]),
                expected: vec![
                    GeonameMatch {
                        geoname: liverpool_city(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: liverpool_metro(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "liverpool",
                match_name_prefix: false,
                filter: Some(vec![uk()]),
                expected: vec![
                    GeonameMatch {
                        geoname: liverpool_city(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: liverpool_metro(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "liverpool",
                match_name_prefix: false,
                filter: Some(vec![liverpool_metro(), england()]),
                expected: vec![
                    GeonameMatch {
                        geoname: liverpool_city(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: liverpool_metro(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "liverpool",
                match_name_prefix: false,
                filter: Some(vec![liverpool_metro(), uk()]),
                expected: vec![
                    GeonameMatch {
                        geoname: liverpool_city(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: liverpool_metro(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "liverpool",
                match_name_prefix: false,
                filter: Some(vec![england(), uk()]),
                expected: vec![
                    GeonameMatch {
                        geoname: liverpool_city(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: liverpool_metro(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "liverpool",
                match_name_prefix: false,
                filter: Some(vec![liverpool_metro(), england(), uk()]),
                expected: vec![
                    GeonameMatch {
                        geoname: liverpool_city(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: liverpool_metro(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            },
            Test {
                query: "gößnitz",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "gössnitz",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "goßnitz",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "gossnitz",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "goessnitz",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: false,
                }],
            },
            Test {
                query: "gö",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "göß",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "gößn",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "gös",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "goß",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "goßn",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "gos",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
            Test {
                query: "goss",
                match_name_prefix: true,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: goessnitz(),
                    match_type: GeonameMatchType::Name,
                    prefix: true,
                }],
            },
        ];

        store.read(|dao| {
            for t in tests {
                let gs = t.filter.clone().unwrap_or_default();
                let gs_refs: Vec<_> = gs.iter().collect();
                let filters = if gs_refs.is_empty() {
                    None
                } else {
                    Some(gs_refs)
                };
                assert_eq!(
                    dao.fetch_geonames(t.query, t.match_name_prefix, filters)?,
                    t.expected,
                    "query={:?} -- Full test: {:?}",
                    t.query,
                    t
                );
            }
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn geonames_metrics() -> anyhow::Result<()> {
        before_each();

        // Add a some records: a core geonames record and some alternates
        // records. The names in each should contribute to metrics.
        let mut store = TestStore::new(
            MockRemoteSettingsClient::default()
                .with_record(geoname_mock_record(
                    "geonames-0",
                    json!([
                        {
                            "id": 4096497,
                            "name": "Waterloo",
                            "feature_class": "P",
                            "feature_code": "PPL",
                            "country": "US",
                            "admin1": "AL",
                            "admin2": "077",
                            "population": 200,
                            "latitude": "34.91814",
                            "longitude": "-88.0642",
                        },
                    ]),
                ))
                .with_record(geoname_alternates_mock_record(
                    "geonames-alternates-0",
                    json!({
                        "language": "en",
                        "names_by_geoname_id": [
                            [4096497, ["a b c d e"]],
                        ],
                    }),
                ))
                .with_record(geoname_alternates_mock_record(
                    "geonames-alternates-1",
                    json!({
                        "language": "en",
                        "names_by_geoname_id": [
                            [1, ["abcdefghik lmnopqrstu"]],
                        ],
                    }),
                )),
        );

        // Ingest weather to also ingest geonames.
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        store.read(|dao| {
            let cache = dao.geoname_cache();
            assert_eq!(cache.max_name_length, 21); // "abcdefghik lmnopqrstu"
            assert_eq!(cache.max_name_word_count, 5); // "a b c d e"
            Ok(())
        })?;

        // Delete the first alternates record. The metrics should change.
        store
            .client_mut()
            .delete_record(geoname_mock_record("geonames-alternates-0", json!({})));
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });
        store.read(|dao| {
            let cache = dao.geoname_cache();
            assert_eq!(cache.max_name_length, 21); // "abcdefghik lmnopqrstu"
            assert_eq!(cache.max_name_word_count, 2); // "abcdefghik lmnopqrstu"
            Ok(())
        })?;

        // Delete the second alternates record. The metrics should change again.
        store
            .client_mut()
            .delete_record(geoname_mock_record("geonames-alternates-1", json!({})));
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });
        store.read(|dao| {
            let cache = dao.geoname_cache();
            assert_eq!(cache.max_name_length, 8); // "waterloo"
            assert_eq!(cache.max_name_word_count, 1); // "waterloo"
            Ok(())
        })?;

        // Add a new record. The metrics should change again.
        store
            .client_mut()
            .add_record(geoname_alternates_mock_record(
                "geonames-alternates-2",
                json!({
                    "language": "en",
                    "names_by_geoname_id": [
                        [2, ["abcd efgh iklm"]],
                    ],
                }),
            ));
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });
        store.read(|dao| {
            let cache = dao.geoname_cache();
            assert_eq!(cache.max_name_length, 14); // "abcd efgh iklm"
            assert_eq!(cache.max_name_word_count, 3); // "abcd efgh iklm"
            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn geonames_deleted_record() -> anyhow::Result<()> {
        before_each();

        // Create the store with the test data and ingest.
        let mut store = new_test_store();
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        // Make sure we have a match.
        store.read(|dao| {
            assert_eq!(
                dao.fetch_geonames("waterloo", false, None)?,
                vec![
                    GeonameMatch {
                        geoname: waterloo_on(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: waterloo_ia(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: waterloo_al(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            );
            Ok(())
        })?;

        // Delete the record.
        store
            .client_mut()
            .delete_record(geoname_mock_record("geonames-0", json!({})));
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        // The same query shouldn't match anymore and the tables should be
        // empty.
        store.read(|dao| {
            assert_eq!(dao.fetch_geonames("waterloo", false, None)?, vec![],);

            let g_ids = dao.conn.query_rows_and_then(
                "SELECT id FROM geonames",
                [],
                |row| -> Result<GeonameId> { Ok(row.get("id")?) },
            )?;
            assert_eq!(g_ids, Vec::<GeonameId>::new());

            let alt_g_ids = dao.conn.query_rows_and_then(
                "SELECT geoname_id FROM geonames_alternates",
                [],
                |row| -> Result<GeonameId> { Ok(row.get("geoname_id")?) },
            )?;
            assert_eq!(alt_g_ids, Vec::<GeonameId>::new());

            Ok(())
        })?;

        Ok(())
    }

    #[test]
    fn geonames_reingest() -> anyhow::Result<()> {
        before_each();

        // Create the store with the test data and ingest.
        let mut store = new_test_store();
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        // Get the table counts.
        let (geonames_count, alternates_count) = store.read(|dao| {
            Ok((
                dao.conn.query_row_and_then(
                    "SELECT count(*) FROM geonames",
                    [],
                    |row| -> Result<i64> { Ok(row.get(0)?) },
                )?,
                dao.conn.query_row_and_then(
                    "SELECT count(*) FROM geonames_alternates",
                    [],
                    |row| -> Result<i64> { Ok(row.get(0)?) },
                )?,
            ))
        })?;

        assert_ne!(geonames_count, 0);
        assert_ne!(alternates_count, 0);

        // Delete the record and add a new record with a new ID that has the
        // same data.
        store
            .client_mut()
            .delete_record(geoname_mock_record("geonames-0", json!({})))
            .add_record(geoname_mock_record("geonames-1", geonames_data()));

        // Ingest again.
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        // Make sure we have a match.
        store.read(|dao| {
            assert_eq!(
                dao.fetch_geonames("waterloo", false, None)?,
                vec![
                    GeonameMatch {
                        geoname: waterloo_on(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: waterloo_ia(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: waterloo_al(),
                        match_type: GeonameMatchType::Name,
                        prefix: false,
                    },
                ],
            );
            Ok(())
        })?;

        // Get the table counts again. They should be the same as before.
        let (new_geonames_count, new_alternates_count) = store.read(|dao| {
            Ok((
                dao.conn.query_row_and_then(
                    "SELECT count(*) FROM geonames",
                    [],
                    |row| -> Result<i64> { Ok(row.get(0)?) },
                )?,
                dao.conn.query_row_and_then(
                    "SELECT count(*) FROM geonames_alternates",
                    [],
                    |row| -> Result<i64> { Ok(row.get(0)?) },
                )?,
            ))
        })?;

        assert_eq!(geonames_count, new_geonames_count);
        assert_eq!(alternates_count, new_alternates_count);

        Ok(())
    }

    #[test]
    fn geonames_store_api() -> anyhow::Result<()> {
        before_each();

        let store = new_test_store();

        // Ingest weather to also ingest geonames.
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        #[derive(Debug)]
        struct Test {
            query: &'static str,
            match_name_prefix: bool,
            filter: Option<Vec<Geoname>>,
            expected: Vec<GeonameMatch>,
        }

        // This only tests a few different calls to exercise all the fetch
        // options. Comprehensive fetch cases are in the main `geonames` test.
        let tests = [
            // simple fetch with no options
            Test {
                query: "ia",
                match_name_prefix: false,
                filter: None,
                expected: vec![GeonameMatch {
                    geoname: ia(),
                    match_type: GeonameMatchType::Abbreviation,
                    prefix: false,
                }],
            },
            // filter
            Test {
                query: "ia",
                match_name_prefix: false,
                filter: Some(vec![waterloo_ia()]),
                expected: vec![GeonameMatch {
                    geoname: ia(),
                    match_type: GeonameMatchType::Abbreviation,
                    prefix: false,
                }],
            },
            // prefix matching
            Test {
                query: "ny",
                match_name_prefix: true,
                filter: None,
                expected: vec![
                    GeonameMatch {
                        geoname: nyc(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                    GeonameMatch {
                        geoname: ny_state(),
                        match_type: GeonameMatchType::Abbreviation,
                        prefix: false,
                    },
                ],
            },
        ];

        for t in tests {
            assert_eq!(
                store.fetch_geonames(t.query, t.match_name_prefix, t.filter.clone()),
                t.expected,
                "Test: {:?}",
                t
            );
        }

        Ok(())
    }
}


use failure;
use std::fs;
use std::io::{Write, self};
use std::fmt::{Write as FmtWrite};
use std::path::PathBuf;
use tempfile;

use rusqlite::{
    Connection,
    OpenFlags,
    Row,
};

use mentat::{
    self,
    Store,
    Keyword,
    errors::Result as MentatResult,
};

#[derive(Debug, Clone)]
struct TransactBuilder {
    counter: u64,
    data: String,
    total_terms: u64,
    terms: u64,
    max_buffer_size: usize
}

impl TransactBuilder {
    #[inline]
    pub fn new_with_size(max_buffer_size: usize) -> Self {
        Self { counter: 0, data: "[\n".into(), terms: 0, total_terms: 0, max_buffer_size }
    }

    #[inline]
    pub fn next_tempid(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }

    #[inline]
    pub fn add_ref_to_tmpid(&mut self, tmpid: u64, attr: &Keyword, ref_tmpid: u64) {
        write!(self.data, " [:db/add \"{}\" {} \"{}\"]\n", tmpid, attr, ref_tmpid).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn add_inst(&mut self, tmpid: u64, attr: &Keyword, micros: i64) {
        write!(self.data, " [:db/add \"{}\" {} #instmicros {}]\n", tmpid, attr, micros).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn add_kw(&mut self, tmpid: u64, attr: &Keyword, val: &Keyword) {
        write!(self.data, " [:db/add \"{}\" {} {}]\n", tmpid, attr, val).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn add_str(&mut self, tmpid: u64, attr: &Keyword, val: &str) {
        // {:?} escapes some chars EDN can't parse (e.g. \'...)
        let s = val.replace("\\", "\\\\").replace("\"", "\\\"");
        write!(self.data, " [:db/add \"{}\" {} \"{}\"]\n", tmpid, attr, s).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn add_long(&mut self, tmpid: u64, attr: &Keyword, val: i64) {
        write!(self.data, " [:db/add \"{}\" {} {}]\n", tmpid, attr, val).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn finish(&mut self) -> &str {
        self.data.push(']');
        &self.data
    }

    #[inline]
    pub fn reset(&mut self) {
        self.terms = 0;
        self.data.clear();
        self.data.push_str("[\n")
    }

    #[inline]
    pub fn should_finish(&self) -> bool {
        self.data.len() >= self.max_buffer_size
    }

    #[inline]
    pub fn maybe_transact(&mut self, store: &mut Store) -> MentatResult<Option<mentat::TxReport>> {
        if self.should_finish() {
            Ok(self.transact(store)?)
        } else {
            Ok(None)
        }
    }

    #[inline]
    pub fn transact(&mut self, store: &mut Store) -> MentatResult<Option<mentat::TxReport>> {
        if self.terms != 0 {
            debug!("\nTransacting {} terms (total = {})", self.terms, self.total_terms);
            let res = store.transact(self.finish());
            if res.is_err() { error!("Error transacting:\n{}", self.data); }
            let report = res?;
            self.reset();
            Ok(Some(report))
        } else {
            Ok(None)
        }
    }
}

lazy_static! {
    static ref PLACE_URL: Keyword = kw!(:place/url);
    static ref PLACE_URL_HASH: Keyword = kw!(:place/url_hash);
    static ref PLACE_TITLE: Keyword = kw!(:place/title);
    static ref PLACE_DESCRIPTION: Keyword = kw!(:place/description);
    static ref PLACE_FRECENCY: Keyword = kw!(:place/frecency);
    static ref VISIT_PLACE: Keyword = kw!(:visit/place);
    static ref VISIT_DATE: Keyword = kw!(:visit/date);
    static ref VISIT_TYPE: Keyword = kw!(:visit/type);

    static ref VISIT_TYPES: Vec<Keyword> = vec![
        kw!(:visit.type/link),
        kw!(:visit.type/typed),
        kw!(:visit.type/bookmark),
        kw!(:visit.type/embed),
        kw!(:visit.type/redirect_permanent),
        kw!(:visit.type/redirect_temporary),
        kw!(:visit.type/download),
        kw!(:visit.type/framed_link),
        kw!(:visit.type/reload),
    ];
}

#[derive(Debug, Clone)]
struct PlaceEntry {
    pub id: i64,
    pub url: String,
    pub url_hash: i64,
    pub description: Option<String>,
    pub title: String,
    pub frecency: i64,
    pub visits: Vec<(i64, &'static Keyword)>,
}

impl PlaceEntry {
    pub fn add(&self, builder: &mut TransactBuilder, store: &mut Store) -> Result<(), failure::Error> {
        let place_id = builder.next_tempid();
        builder.add_str(place_id, &*PLACE_URL, &self.url);
        builder.add_long(place_id, &*PLACE_URL_HASH, self.url_hash);
        builder.add_str(place_id, &*PLACE_TITLE, &self.title);
        if let Some(desc) = &self.description {
            builder.add_str(place_id, &*PLACE_DESCRIPTION, desc);
        }

        builder.add_long(place_id, &*PLACE_FRECENCY, self.frecency);

        assert!(self.visits.len() > 0);

        if builder.max_buffer_size == 0 {
            let report = builder.transact(store)?.unwrap();
            let place_eid = report.tempids.get(&format!("{}", place_id)).unwrap();
            // One transaction per visit.
            for (microtime, visit_type) in &self.visits {
                let visit_id = builder.next_tempid();
                builder.add_long(visit_id, &*VISIT_PLACE, *place_eid);
                builder.add_inst(visit_id, &*VISIT_DATE, *microtime);
                builder.add_kw(visit_id, &*VISIT_TYPE, visit_type);
                builder.transact(store)?;
            }
        } else {
            for (microtime, visit_type) in &self.visits {
                let visit_id = builder.next_tempid();
                builder.add_ref_to_tmpid(visit_id, &*VISIT_PLACE, place_id);
                builder.add_inst(visit_id, &*VISIT_DATE, *microtime);
                builder.add_kw(visit_id, &*VISIT_TYPE, visit_type);
            }
            builder.maybe_transact(store)?;
        }
        Ok(())
    }

    pub fn from_row(row: &Row) -> PlaceEntry {
        let transition_type: i64 = row.get("visit_type");
        PlaceEntry {
            id: row.get("place_id"),
            url: row.get("place_url"),
            url_hash: row.get("place_url_hash"),
            description: row.get("place_description"),
            title: row.get::<_, Option<String>>("place_title").unwrap_or("".into()),
            frecency: row.get("place_frecency"),
            visits: vec![(row.get("visit_date"), &VISIT_TYPES[(transition_type as usize).saturating_sub(1)])],
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlacesToMentat {
    pub mentat_db_path: PathBuf,
    pub places_db_path: PathBuf,
    pub one_tx_per_visit: bool,
}


static SCHEMA: &'static str = include_str!("places-schema.edn");


impl PlacesToMentat {
    pub fn run(self) -> Result<(), failure::Error> {

        debug!("Copying places.sqlite to a temp file for reading");
        let temp_dir = tempfile::tempdir()?;
        let temp_places_path = temp_dir.path().join("places.sqlite");

        fs::copy(&self.places_db_path, &temp_places_path)?;
        let places = Connection::open_with_flags(&temp_places_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        let mut store = Store::open_empty(self.mentat_db_path.to_str().unwrap())?;

        debug!("Transacting initial schema");
        store.transact(SCHEMA)?;

        let mut stmt = places.prepare("
            SELECT
                p.id          as place_id,
                p.url         as place_url,
                p.url_hash    as place_url_hash,
                p.description as place_description,
                p.title       as place_title,
                p.frecency    as place_frecency,
                v.visit_date  as visit_date,
                v.visit_type  as visit_type
            FROM moz_places p
            JOIN moz_historyvisits v
                ON p.id = v.place_id
            ORDER BY p.id
        ").unwrap();

        let (place_count, visit_count) = {
            let mut stmt = places.prepare("select count(*) from moz_places").unwrap();
            let mut rows = stmt.query(&[]).unwrap();
            let ps: i64 = rows.next().unwrap()?.get(0);

            let mut stmt = places.prepare("select count(*) from moz_historyvisits").unwrap();
            let mut rows = stmt.query(&[]).unwrap();
            let vs: i64 = rows.next().unwrap()?.get(0);
            (ps, vs)
        };

        println!("Querying {} places ({} visits)", place_count, visit_count);

        let mut current_place = PlaceEntry {
            id: -1,
            url: "".into(),
            url_hash: 0,
            description: None,
            title: "".into(),
            frecency: 0,
            visits: vec![],
        };

        let max_buffer_size = if self.one_tx_per_visit { 0 } else { 1024 * 1024 * 1024 * 1024 };

        let mut builder = TransactBuilder::new_with_size(max_buffer_size);

        let mut so_far = 0;
        let mut rows = stmt.query(&[])?;

        while let Some(row_or_error) = rows.next() {
            let row = row_or_error?;
            let id: i64 = row.get("place_id");
            if current_place.id == id {
                let tty: i64 = row.get("visit_type");
                current_place.visits.push((
                    row.get("visit_date"),
                    &VISIT_TYPES.get((tty.max(0) as usize).saturating_sub(1))
                        .unwrap_or_else(|| &VISIT_TYPES[0])
                ));
                continue;
            }

            if current_place.id >= 0 {
                current_place.add(&mut builder, &mut store)?;
                // builder.maybe_transact(&mut store)?;
                print!("\rProcessing {} / {} places (approx.)", so_far, place_count);
                io::stdout().flush()?;
                so_far += 1;
            }
            current_place = PlaceEntry::from_row(&row);
        }

        if current_place.id >= 0 {
            current_place.add(&mut builder, &mut store)?;
            // builder.maybe_transact(&mut store)?;
            println!("\rProcessing {} / {} places (approx.)", so_far + 1, place_count);
        }
        builder.transact(&mut store)?;
        println!("Done!");
        Ok(())
    }
}


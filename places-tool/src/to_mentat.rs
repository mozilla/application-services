
use failure;
use std::fs;
use std::io::{Write, self};
use std::fmt::{Write as FmtWrite};
use std::path::PathBuf;
use tempfile;
use rand::prelude::*;

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
    pub fn next_tempid(&mut self) -> String {
        self.counter += 1;
        self.counter.to_string()
    }

    #[inline]
    pub fn add_ref_to_tmpid(&mut self, tmpid: &str, attr: &Keyword, ref_tmpid: &str) {
        write!(self.data, " [:db/add {:?} {} {:?}]\n", tmpid, attr, ref_tmpid).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn add_ref_to_lookup_ref_long(&mut self,
                                      tmpid: &str, attr: &Keyword,
                                      lookup_ref_attr: &Keyword, lookup_ref_val: i64) {
        write!(self.data, " [:db/add {:?} {} (lookup-ref {} {})]\n",
            tmpid, attr, lookup_ref_attr, lookup_ref_val).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn add_inst(&mut self, tmpid: &str, attr: &Keyword, micros: i64) {
        write!(self.data, " [:db/add {:?} {} #instmicros {}]\n", tmpid, attr, micros).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn add_str(&mut self, tmpid: &str, attr: &Keyword, val: &str) {
        // {:?} escapes some chars EDN can't parse (e.g. \'...)
        let s = val.replace("\\", "\\\\").replace("\"", "\\\"");
        write!(self.data, " [:db/add {:?} {} \"{}\"]\n", tmpid, attr, s).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn add_long(&mut self, tmpid: &str, attr: &Keyword, val: i64) {
        write!(self.data, " [:db/add {:?} {} {}]\n", tmpid, attr, val).unwrap();
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

    static ref ORIGIN_PREFIX: Keyword = kw!(:origin/prefix);
    static ref ORIGIN_HOST: Keyword = kw!(:origin/host);
    static ref ORIGIN_PLACES_ID: Keyword = kw!(:origin/places_id);
    

    static ref PAGE_URL: Keyword = kw!(:page/url);
    static ref PAGE_ORIGIN: Keyword = kw!(:page/origin);

    static ref PAGE_META_TITLE: Keyword = kw!(:page_meta/title);
    // static ref PAGE_META_FAVICON_URL: Keyword = kw!(:page_meta/favicon_url);
    static ref PAGE_META_DESCRIPTION: Keyword = kw!(:page_meta/description);
    static ref PAGE_META_PREVIEW_IMAGE_URL: Keyword = kw!(:page_meta/preview_image_url);

    // static ref CONTEXT_DEVICE: Keyword = kw!(:context/device);
    // static ref CONTEXT_CONTAINER: Keyword = kw!(:context/container);
    static ref CONTEXT_ID: Keyword = kw!(:context/id);

    static ref VISIT_PAGE_META: Keyword = kw!(:visit/page_meta);
    static ref VISIT_CONTEXT: Keyword = kw!(:visit/context);
    static ref VISIT_PAGE: Keyword = kw!(:visit/page);
    static ref VISIT_DATE: Keyword = kw!(:visit/date);

    static ref VISIT_SOURCE_VISIT: Keyword = kw!(:visit/source_visit);

    // static ref VISIT_SOURCE_REDIRECT: Keyword = kw!(:visit/source_redirect);
    // static ref VISIT_SOURCE_BOOKMARK: Keyword = kw!(:visit/source_bookmark);

    // Only used in `initial-data.edn`
    //
    // static ref DEVICE_NAME: Keyword = kw!(:device/name)
    // static ref DEVICE_TYPE: Keyword = kw!(:device/type)
    // static ref DEVICE_TYPE_DESKTOP: Keyword = kw!(:device.type/desktop)
    // static ref DEVICE_TYPE_MOBILE: Keyword = kw!(:device.type/mobile)
    // static ref CONTAINER_NAME: Keyword = kw!(:container/name)

}

const MAX_CONTEXT_ID: i64 = 4;


#[derive(Debug, Clone, Default)]
struct VisitInfo {
    // Everything else we fabricate (for reasons).
    date: i64,
}

#[derive(Debug, Clone, Default)]
struct PlaceEntry {
    pub id: i64,
    pub url: String,
    pub description: Option<String>,
    pub preview_image_url: Option<String>,
    pub title: String,
    pub origin_id: i64,
    pub visits: Vec<VisitInfo>,
}

impl PlaceEntry {
    pub fn add(&self, builder: &mut TransactBuilder, store: &mut Store) -> Result<(), failure::Error> {
        let page_id = builder.next_tempid();
        builder.add_str(&page_id, &*PAGE_URL, &self.url);
        builder.add_ref_to_lookup_ref_long(&page_id, &*PAGE_ORIGIN, &*ORIGIN_PLACES_ID, self.origin_id);

        let page_meta_id = builder.next_tempid();

        builder.add_str(&page_meta_id, &*PAGE_META_TITLE, &self.title);
        if let Some(desc) = &self.description {
            builder.add_str(&page_meta_id, &*PAGE_META_DESCRIPTION, &desc);
        }
        if let Some(preview) = &self.preview_image_url {
            builder.add_str(&page_meta_id, &*PAGE_META_PREVIEW_IMAGE_URL, &preview);
        }

        let mut rng = thread_rng();
        for visit in &self.visits {
            let visit_id = builder.next_tempid();
            builder.add_ref_to_tmpid(&visit_id, &*VISIT_PAGE, &page_id);
            builder.add_ref_to_tmpid(&visit_id, &*VISIT_PAGE_META, &page_meta_id);
            // unwrap is safe, only None for an empty slice.
            builder.add_ref_to_lookup_ref_long(&visit_id, &*VISIT_CONTEXT,
                                               &*CONTEXT_ID,
                                               rng.gen_range(0, MAX_CONTEXT_ID));
            builder.add_inst(&visit_id, &*VISIT_DATE, visit.date);
            // Point the visit at itself. This doesn't really matter, but
            // pointing at another visit would require us keep a huge hashmap in
            // memory, or to keep the places id on the visit as a unique
            // identity which we use as a lookup ref, which will effect the db
            // size a lot in a way we wouldn't need to in reality.
            builder.add_ref_to_tmpid(&visit_id, &*VISIT_SOURCE_VISIT, &visit_id);
        }
        // not one tx per visit anymore (and doing per place instead) because
        // the bookkeeping/separation required is too annoying.
        builder.maybe_transact(store)?;
        Ok(())
    }

    pub fn from_row(row: &Row) -> PlaceEntry {
        PlaceEntry {
            id: row.get("place_id"),
            url: row.get("place_url"),
            origin_id: row.get("place_origin_id"),
            description: row.get("place_description"),
            preview_image_url: row.get("place_preview_image_url"),
            title: row.get::<_, Option<String>>("place_title").unwrap_or("".into()),
            visits: vec![VisitInfo { date: row.get("visit_date") }],
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlacesToMentat {
    pub mentat_db_path: PathBuf,
    pub places_db_path: PathBuf,
    pub realistic: bool,
}

static SCHEMA: &'static str = include_str!("places-schema.edn");
static INITIAL_DATA: &'static str = include_str!("initial-data.edn");

impl PlacesToMentat {
    pub fn run(self) -> Result<(), failure::Error> {

        debug!("Copying places.sqlite to a temp file for reading");
        let temp_dir = tempfile::tempdir()?;
        let temp_places_path = temp_dir.path().join("places.sqlite");

        fs::copy(&self.places_db_path, &temp_places_path)?;
        let places = Connection::open_with_flags(&temp_places_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        // New versions of mentat kill open_empty, and we already know this is empty.
        let mut store = Store::open(self.mentat_db_path.to_str().unwrap())?;

        debug!("Transacting initial schema");
        store.transact(SCHEMA)?;
        store.transact(INITIAL_DATA)?;
        
        let max_buffer_size = if self.realistic { 0 } else { 1024 * 1024 * 1024 * 1024 };
        let mut builder = TransactBuilder::new_with_size(max_buffer_size);

        {
            let mut origins_stmt = places.prepare("SELECT id, prefix, host FROM moz_origins")?;
            let origins = origins_stmt.query_map(&[], |row| {
                (row.get::<_, i64>("id"),
                 row.get::<_, String>("prefix"),
                 row.get::<_, String>("host"))
            })?.collect::<Result<Vec<_>, _>>()?;

            println!("Adding {} origins...", origins.len());
            for (id, prefix, host) in origins {
                let tmpid = builder.next_tempid();
                builder.add_long(&tmpid, &*ORIGIN_PLACES_ID, id);
                builder.add_str(&tmpid, &*ORIGIN_PREFIX, &host);
                builder.add_str(&tmpid, &*ORIGIN_HOST, &prefix);
                builder.maybe_transact(&mut store)?;
            }
            // Force a transaction so that lookup refs work.
            builder.transact(&mut store)?;
        }

        let (place_count, visit_count) = {
            let mut stmt = places.prepare("SELECT count(*) FROM moz_places").unwrap();
            let mut rows = stmt.query(&[]).unwrap();
            let ps: i64 = rows.next().unwrap()?.get(0);

            let mut stmt = places.prepare("SELECT count(*) FROM moz_historyvisits").unwrap();
            let mut rows = stmt.query(&[]).unwrap();
            let vs: i64 = rows.next().unwrap()?.get(0);
            (ps, vs)
        };

        println!("Querying {} places ({} visits)", place_count, visit_count);

        let mut stmt = places.prepare("
            SELECT
                p.id                as place_id,
                p.url               as place_url,
                p.description       as place_description,
                p.preview_image_url as place_preview_image_url,
                p.title             as place_title,
                p.origin_id         as place_origin_id,
                v.visit_date        as visit_date
            FROM moz_places p
            JOIN moz_historyvisits v
                ON p.id = v.place_id
            ORDER BY p.id
        ")?;

        let mut current_place = PlaceEntry { id: -1, .. PlaceEntry::default() };

        let mut so_far = 0;
        let mut rows = stmt.query(&[])?;

        while let Some(row_or_error) = rows.next() {
            let row = row_or_error?;
            let id: i64 = row.get("place_id");
            if current_place.id == id {
                current_place.visits.push(VisitInfo { date: row.get("visit_date") });
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

        println!("Vacuuming mentat DB");

        let mentat_sqlite_conn = store.dismantle().0;
        mentat_sqlite_conn.execute("VACUUM", &[])?;
        println!("Done!");
        Ok(())
    }

}


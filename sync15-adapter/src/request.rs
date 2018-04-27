/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use util::ServerTimestamp;
use bso_record::{BsoRecord, EncryptedPayload};

use serde_json;
use std::fmt::{self, Write};
use std::collections::HashMap;
use url::{Url, UrlQuery, form_urlencoded::Serializer};
use error::{self, Result};
use hyper::{StatusCode};
use reqwest::Response;


#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RequestOrder { Oldest, Newest, Index }

header! { (XIfUnmodifiedSince, "X-If-Unmodified-Since") => [ServerTimestamp] }
header! { (XLastModified, "X-Last-Modified") => [ServerTimestamp] }
header! { (XWeaveTimestamp, "X-Weave-Timestamp") => [ServerTimestamp] }

impl fmt::Display for RequestOrder {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &RequestOrder::Oldest => f.write_str("oldest"),
            &RequestOrder::Newest => f.write_str("newest"),
            &RequestOrder::Index => f.write_str("index")
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollectionRequest {
    pub collection: String,
    pub full: bool,
    pub ids: Option<Vec<String>>,
    pub limit: usize,
    pub older: Option<ServerTimestamp>,
    pub newer: Option<ServerTimestamp>,
    pub order: Option<RequestOrder>,
    pub commit: bool,
    pub batch: Option<String>,
}

impl CollectionRequest {
    #[inline]
    pub fn new<S>(collection: S) -> CollectionRequest where S: Into<String> {
        CollectionRequest {
            collection: collection.into(),
            full: false,
            ids: None,
            limit: 0,
            older: None,
            newer: None,
            order: None,
            commit: false,
            batch: None,
        }
    }

    #[inline]
    pub fn ids<V>(&mut self, v: V) -> &mut CollectionRequest where V: Into<Vec<String>> {
        self.ids = Some(v.into());
        self
    }

    #[inline]
    pub fn full(&mut self) -> &mut CollectionRequest {
        self.full = true;
        self
    }

    #[inline]
    pub fn older_than(&mut self, ts: ServerTimestamp) -> &mut CollectionRequest {
        self.older = Some(ts);
        self
    }

    #[inline]
    pub fn newer_than(&mut self, ts: ServerTimestamp) -> &mut CollectionRequest {
        self.newer = Some(ts);
        self
    }

    #[inline]
    pub fn sort_by(&mut self, order: RequestOrder) -> &mut CollectionRequest {
        self.order = Some(order);
        self
    }

    #[inline]
    pub fn limit(&mut self, num: usize) -> &mut CollectionRequest {
        self.limit = num;
        self
    }

    #[inline]
    pub fn batch(&mut self, batch: Option<String>) -> &mut CollectionRequest {
        self.batch = batch;
        self
    }

    #[inline]
    pub fn commit(&mut self, v: bool) -> &mut CollectionRequest {
        self.commit = v;
        self
    }

    fn build_query(&self, pairs: &mut Serializer<UrlQuery>) {
        if self.full {
            pairs.append_pair("full", "1");
        }
        if self.limit > 0 {
            pairs.append_pair("limit", &format!("{}", self.limit));
        }
        if let &Some(ref ids) = &self.ids {
            pairs.append_pair("ids", &ids.join(","));
        }
        if let &Some(ref batch) = &self.batch {
            pairs.append_pair("batch", &batch);
        }
        if self.commit {
            pairs.append_pair("commit", "true");
        }
        if let Some(ts) = self.older {
            pairs.append_pair("older", &format!("{}", ts));
        }
        if let Some(ts) = self.newer {
            pairs.append_pair("newer", &format!("{}", ts));
        }
        if let Some(o) = self.order {
            pairs.append_pair("sort", &format!("{}", o));
        }
        pairs.finish();
    }

    pub fn build_url(&self, mut base_url: Url) -> Result<Url> {
        base_url.path_segments_mut()
                .map_err(|_| error::unexpected("Not base URL??"))?
                .extend(&["storage", &self.collection]);
        self.build_query(&mut base_url.query_pairs_mut());
        // This is strange but just accessing query_pairs_mut makes you have
        // a trailing question mark on your url. I don't think anything bad
        // would happen here, but I don't know, and also, it looks dumb so
        // I'd rather not have it.
        if base_url.query() == Some("") {
            base_url.set_query(None);
        }
        Ok(base_url)
    }
}

/// Manages a pair of (byte, count) limits for a PostQueue, such as
/// (max_post_bytes, max_post_records) or (max_total_bytes, max_total_records).
#[derive(Debug, Clone)]
struct LimitTracker {
    max_bytes: usize,
    max_records: usize,
    cur_bytes: usize,
    cur_records: usize,
}

impl LimitTracker {
    pub fn new(max_bytes: usize, max_records: usize) -> LimitTracker {
        LimitTracker {
            max_bytes,
            max_records,
            cur_bytes: 0,
            cur_records: 0
        }
    }

    pub fn clear(&mut self) {
        self.cur_records = 0;
        self.cur_bytes = 0;
    }

    pub fn can_add_record(&self, payload_size: usize) -> bool {
        // Desktop does the cur_bytes check as exclusive, but we shouldn't see any servers that
        // don't have https://github.com/mozilla-services/server-syncstorage/issues/73
        self.cur_records + 1 <= self.max_records &&
        self.cur_bytes + payload_size <= self.max_bytes
    }

    pub fn can_never_add(&self, record_size: usize) -> bool {
        record_size >= self.max_bytes
    }

    pub fn record_added(&mut self, record_size: usize) {
        assert!(self.can_add_record(record_size),
                "LimitTracker::record_added caller must check can_add_record");
        self.cur_records += 1;
        self.cur_bytes += 1;
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct InfoConfiguration {
    /// The maximum size in bytes of the overall HTTP request body that will be accepted by the
    /// server.
    #[serde(default = "default_max_request_bytes")]
    pub max_request_bytes: usize,

    /// The maximum number of records that can be uploaded to a collection in a single POST request.
    #[serde(default = "usize::max_value")]
    pub max_post_records: usize,

    /// The maximum combined size in bytes of the record payloads that can be uploaded to a
    /// collection in a single POST request.
    #[serde(default = "usize::max_value")]
    pub max_post_bytes: usize,

    /// The maximum total number of records that can be uploaded to a collection as part of a
    /// batched upload.
    #[serde(default = "usize::max_value")]
    pub max_total_records: usize,

    /// The maximum total combined size in bytes of the record payloads that can be uploaded to a
    /// collection as part of a batched upload.
    #[serde(default = "usize::max_value")]
    pub max_total_bytes: usize,

    /// The maximum size of an individual BSO payload, in bytes.
    #[serde(default = "default_max_record_payload_bytes")]
    pub max_record_payload_bytes: usize,
}

// This is annoying but seems to be the only way to do it.
fn default_max_request_bytes() -> usize { 260 * 1024 }
fn default_max_record_payload_bytes() -> usize { 256 * 1024 }

#[derive(Debug, Clone, Deserialize)]
pub struct UploadResult {
    batch: Option<String>,
    /// Maps record id => why failde
    pub failed: HashMap<String, String>,
    /// Vec of ids
    pub success: Vec<String>
}

// Easier to fake during tests
#[derive(Debug, Clone)]
pub struct PostResponse {
    pub status: StatusCode,
    pub result: UploadResult, // This is lazy...
    pub last_modified: ServerTimestamp,
}

impl PostResponse {
    pub fn from_response(r: &mut Response) -> Result<PostResponse> {
        let result: UploadResult = r.json()?;
        let last_modified = r.headers().get::<XLastModified>().map(|h| **h).ok_or_else(||
            error::unexpected("Server didn't send X-Last-Modified header"))?;
        let status = r.status();
        Ok(PostResponse { status, result, last_modified })
    }
}


#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum BatchState {
    Unsupported,
    NoBatch,
    InBatch(String),
}

#[derive(Debug)]
pub struct PostQueue<Post, OnResponse> {
    poster: Post,
    on_response: OnResponse,
    post_limits: LimitTracker,
    batch_limits: LimitTracker,
    max_payload_bytes: usize,
    max_request_bytes: usize,
    queued: Vec<u8>,
    batch: BatchState,
    last_modified: ServerTimestamp,
}

pub trait BatchPoster {
    fn post(&mut self,
            body: &[u8],
            ts: ServerTimestamp,
            batch: Option<String>,
            commit: bool) -> Result<PostResponse>;
}

/// The Poster param takes
/// 1. A slice that will represent the request body
/// 2. XIUS value for the request
/// 3. An optional batch id, which will be None if batching is not supported.
/// 4. A boolean for whether or not this is a batch commit.
///
/// Note: Poster should not report non-success HTTP statuses as errors!!
impl<Poster, OnResponse> PostQueue<Poster, OnResponse>
where
    Poster: BatchPoster,
    OnResponse: FnMut(PostResponse, bool) -> Result<()>
{
    pub fn new(config: &InfoConfiguration, ts: ServerTimestamp, poster: Poster, on_response: OnResponse) -> PostQueue<Poster, OnResponse> {
        PostQueue {
            poster,
            on_response,
            last_modified: ts,
            post_limits: LimitTracker::new(config.max_post_bytes, config.max_post_records),
            batch_limits: LimitTracker::new(config.max_total_bytes, config.max_total_records),
            batch: BatchState::NoBatch,
            max_payload_bytes: config.max_record_payload_bytes,
            max_request_bytes: config.max_request_bytes,
            queued: Vec::new(),
        }
    }

    #[inline]
    fn in_batch(&self) -> bool {
        match &self.batch {
            &BatchState::Unsupported |
            &BatchState::NoBatch => true,
            _ => false
        }
    }

    pub fn enqueue(&mut self, record: &BsoRecord<EncryptedPayload>) -> Result<bool> {
        let payload_length = record.payload.serialized_len();

        if self.post_limits.can_never_add(payload_length) ||
           self.batch_limits.can_never_add(payload_length) ||
           payload_length >= self.max_payload_bytes {
            warn!("Single record too large to submit to server ({} b)", payload_length);
            return Ok(false);
        }

        // Write directly into `queued` but undo if necessary (the vast majority of the time
        // it won't be necessary). If we hit a problem we need to undo that, but the only error
        // case we have to worry about right now is in flush()
        let item_start = self.queued.len();

        // This is conservative but can't hurt.
        self.queued.reserve(payload_length + 2);

        // Either the first character in an array, or a comma separating
        // it from the previous item.
        let c = if self.queued.is_empty() { b'[' } else { b',' };
        self.queued.push(c);

        // This unwrap is fine, since serde_json's failure case is HashMaps that have non-object
        // keys, which is impossible. If you decide to change this part, you *need* to call
        // `self.queued.truncate(item_start)` here in the failure case!
        serde_json::to_writer(&mut self.queued, &record).unwrap();

        let item_end = self.queued.len();

        debug_assert!(item_end >= payload_length,
                      "EncryptedPayload::serialized_len is bugged");

        // The + 1 is only relevant for the final record, which will have a trailing ']'.
        let item_len = item_end - item_start + 1;

        if item_len >= self.max_request_bytes {
            self.queued.truncate(item_start);
            warn!("Single record too large to submit to server ({} b)", item_len);
            return Ok(false);
        }

        let can_post_record = self.post_limits.can_add_record(payload_length);
        let can_batch_record = self.batch_limits.can_add_record(payload_length);
        let can_send_record = self.queued.len() < self.max_request_bytes;

        if !can_post_record || !can_send_record || !can_batch_record {
            debug!("PostQueue flushing! (can_post = {}, can_send = {}, can_batch = {})",
                   can_post_record, can_send_record, can_batch_record);
            // "unwrite" the record.
            self.queued.truncate(item_start);
            // Flush whatever we have queued.
            self.flush(!can_batch_record)?;
            // And write it again.
            let c = if self.queued.is_empty() { b'[' } else { b',' };
            self.queued.push(c);
            serde_json::to_writer(&mut self.queued, &record).unwrap();
        }

        self.post_limits.record_added(payload_length);
        self.batch_limits.record_added(payload_length);

        Ok(true)
    }

    pub fn flush(&mut self, want_commit: bool) -> Result<()> {
        if self.queued.len() == 0 {
            assert!(!self.in_batch(),
                    "Bug: Somehow we're in a batch but have no queued records");
            // Nothing to do!
            return Ok(());
        }

        self.queued.push(b']');
        let batch_id = match &self.batch {
            // Not the first post and we know we have no batch semantics.
            &BatchState::Unsupported => None,
            // First commit in possible batch
            &BatchState::NoBatch => Some("true".into()),
            // In a batch and we have a batch id.
            &BatchState::InBatch(ref s) => Some(s.clone())
        };

        info!("Posting {} records of {} bytes", self.post_limits.cur_records, self.queued.len());

        let is_commit = want_commit && !batch_id.is_none();
        // Weird syntax for calling a function object that is a property.
        let resp_or_error = self.poster.post(&self.queued, self.last_modified, batch_id, is_commit);

        self.queued.truncate(0);

        if want_commit {
            self.batch_limits.clear();
        }
        self.post_limits.clear();

        let resp = resp_or_error?;

        if !resp.status.is_success() {
            (self.on_response)(resp, !want_commit)?;
            bail!(error::unexpected("Expected OnResponse to have bailed out!"));
        }

        if want_commit {
            debug!("Committed batch {:?}", self.batch);
            self.batch = BatchState::NoBatch;
            self.last_modified = resp.last_modified;
            (self.on_response)(resp, false)?;
            return Ok(());
        }

        if resp.status != StatusCode::Accepted {
            if self.in_batch() {
                bail!(error::unexpected(
                    "Server responded non-202 success code while a batch was in progress"));
            }
            self.last_modified = resp.last_modified;
            self.batch = BatchState::Unsupported;
            (self.on_response)(resp, false)?;
            return Ok(());
        }

        let batch_id = resp.result.batch.as_ref().ok_or_else(||
            error::unexpected("Invalid server response: 202 without a batch ID"))?.clone();

        match &self.batch {
            &BatchState::Unsupported => {
                warn!("Server changed it's mind about supporting batching mid-batch...");
            },

            &BatchState::InBatch(ref cur_id) => {
                if cur_id != &batch_id {
                    bail!(error::unexpected("Server changed batch id mid-batch!"));
                }
            },
            _ => {}
        }

        // Can't change this in match arms without NLL
        self.batch = BatchState::InBatch(batch_id);
        self.last_modified = resp.last_modified;

        (self.on_response)(resp, false)?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_url_building() {
        let base = Url::parse("https://example.com/sync").unwrap();
        let empty = CollectionRequest::new("foo").build_url(base.clone()).unwrap();
        assert_eq!(empty.as_str(), "https://example.com/sync/storage/foo");
        let batch_start = CollectionRequest::new("bar").batch(Some("true".into())).commit(false)
                                                       .build_url(base.clone()).unwrap();
        assert_eq!(batch_start.as_str(), "https://example.com/sync/storage/bar?batch=true");
        let batch_commit = CollectionRequest::new("asdf").batch(Some("1234abc".into())).commit(true)
                                                         .build_url(base.clone())
                                                         .unwrap();
        assert_eq!(batch_commit.as_str(),
            "https://example.com/sync/storage/asdf?batch=1234abc&commit=true");

        let idreq = CollectionRequest::new("wutang").full().ids(vec!["rza".into(), "gza".into()])
                                                    .build_url(base.clone()).unwrap();
        assert_eq!(idreq.as_str(), "https://example.com/sync/storage/wutang?full=1&ids=rza%2Cgza");

        let complex = CollectionRequest::new("specific").full().limit(10).sort_by(RequestOrder::Oldest)
                                                        .older_than(ServerTimestamp(9876.54))
                                                        .newer_than(ServerTimestamp(1234.56))
                                                        .build_url(base.clone()).unwrap();
        assert_eq!(complex.as_str(),
            "https://example.com/sync/storage/specific?full=1&limit=10&older=9876.54&newer=1234.56&sort=oldest");

    }
}

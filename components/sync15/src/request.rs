/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::bso_record::EncryptedBso;
use crate::error::{self, ErrorKind, Result};
use crate::util::ServerTimestamp;
use serde_derive::*;
use std::collections::HashMap;
use std::default::Default;
use std::fmt;
use std::ops::Deref;
use url::{form_urlencoded::Serializer, Url, UrlQuery};
use viaduct::{header_names, status_codes, Response};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RequestOrder {
    Oldest,
    Newest,
    Index,
}

impl fmt::Display for RequestOrder {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequestOrder::Oldest => f.write_str("oldest"),
            RequestOrder::Newest => f.write_str("newest"),
            RequestOrder::Index => f.write_str("index"),
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
    pub fn new<S>(collection: S) -> CollectionRequest
    where
        S: Into<String>,
    {
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
    pub fn ids<V>(mut self, v: V) -> CollectionRequest
    where
        V: Into<Vec<String>>,
    {
        self.ids = Some(v.into());
        self
    }

    #[inline]
    pub fn full(mut self) -> CollectionRequest {
        self.full = true;
        self
    }

    #[inline]
    pub fn older_than(mut self, ts: ServerTimestamp) -> CollectionRequest {
        self.older = Some(ts);
        self
    }

    #[inline]
    pub fn newer_than(mut self, ts: ServerTimestamp) -> CollectionRequest {
        self.newer = Some(ts);
        self
    }

    #[inline]
    pub fn sort_by(mut self, order: RequestOrder) -> CollectionRequest {
        self.order = Some(order);
        self
    }

    #[inline]
    pub fn limit(mut self, num: usize) -> CollectionRequest {
        self.limit = num;
        self
    }

    #[inline]
    pub fn batch(mut self, batch: Option<String>) -> CollectionRequest {
        self.batch = batch;
        self
    }

    #[inline]
    pub fn commit(mut self, v: bool) -> CollectionRequest {
        self.commit = v;
        self
    }

    fn build_query(&self, pairs: &mut Serializer<UrlQuery<'_>>) {
        if self.full {
            pairs.append_pair("full", "1");
        }
        if self.limit > 0 {
            pairs.append_pair("limit", &format!("{}", self.limit));
        }
        if let Some(ids) = &self.ids {
            pairs.append_pair("ids", &ids.join(","));
        }
        if let Some(batch) = &self.batch {
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
        base_url
            .path_segments_mut()
            .map_err(|_| ErrorKind::UnacceptableUrl("Storage server URL is not a base".into()))?
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
            cur_records: 0,
        }
    }

    pub fn clear(&mut self) {
        self.cur_records = 0;
        self.cur_bytes = 0;
    }

    pub fn can_add_record(&self, payload_size: usize) -> bool {
        // Desktop does the cur_bytes check as exclusive, but we shouldn't see any servers that
        // don't have https://github.com/mozilla-services/server-syncstorage/issues/73
        self.cur_records < self.max_records && self.cur_bytes + payload_size <= self.max_bytes
    }

    pub fn can_never_add(&self, record_size: usize) -> bool {
        record_size >= self.max_bytes
    }

    pub fn record_added(&mut self, record_size: usize) {
        assert!(
            self.can_add_record(record_size),
            "LimitTracker::record_added caller must check can_add_record"
        );
        self.cur_records += 1;
        self.cur_bytes += record_size;
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

// This is annoying but seems to be the only way to do it...
fn default_max_request_bytes() -> usize {
    260 * 1024
}
fn default_max_record_payload_bytes() -> usize {
    256 * 1024
}

impl Default for InfoConfiguration {
    #[inline]
    fn default() -> InfoConfiguration {
        InfoConfiguration {
            max_request_bytes: default_max_request_bytes(),
            max_record_payload_bytes: default_max_record_payload_bytes(),
            max_post_records: usize::max_value(),
            max_post_bytes: usize::max_value(),
            max_total_records: usize::max_value(),
            max_total_bytes: usize::max_value(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct InfoCollections(HashMap<String, ServerTimestamp>);

impl InfoCollections {
    pub fn new(collections: HashMap<String, ServerTimestamp>) -> InfoCollections {
        InfoCollections(collections)
    }
}

impl Deref for InfoCollections {
    type Target = HashMap<String, ServerTimestamp>;

    fn deref(&self) -> &HashMap<String, ServerTimestamp> {
        &self.0
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadResult {
    batch: Option<String>,
    /// Maps record id => why failed
    #[serde(default = "HashMap::new")]
    pub failed: HashMap<String, String>,
    /// Vec of ids
    #[serde(default = "Vec::new")]
    pub success: Vec<String>,
}

// Easier to fake during tests
#[derive(Debug, Clone)]
pub struct PostResponse {
    pub status: u16,
    pub result: UploadResult, // This is lazy...
    pub last_modified: ServerTimestamp,
}

impl PostResponse {
    pub fn is_success(&self) -> bool {
        status_codes::is_success_code(self.status)
    }
    pub fn from_response(r: &Response) -> Result<PostResponse> {
        let result: UploadResult = r.json()?;
        // TODO Can this happen in error cases?
        let last_modified = r
            .headers
            .try_get::<ServerTimestamp, _>(header_names::X_LAST_MODIFIED)
            .ok_or_else(|| ErrorKind::MissingServerTimestamp)?;
        let status = r.status;
        Ok(PostResponse {
            status,
            result,
            last_modified,
        })
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
    /// Note: Last argument (reference to the batch poster) is provided for the purposes of testing
    /// Important: Poster should not report non-success HTTP statuses as errors!!
    fn post<P, O>(
        &self,
        body: Vec<u8>,
        xius: ServerTimestamp,
        batch: Option<String>,
        commit: bool,
        queue: &PostQueue<P, O>,
    ) -> Result<PostResponse>;
}

// We don't just use a FnMut here since we want to override it in mocking for RefCell<TestType>,
// which we can't do for FnMut since neither FnMut nor RefCell are defined here. Also, this
// is somewhat better for documentation.
pub trait PostResponseHandler {
    fn handle_response(&mut self, r: PostResponse, mid_batch: bool) -> Result<()>;
}

#[derive(Debug, Clone)]
pub(crate) struct NormalResponseHandler {
    pub failed_ids: Vec<String>,
    pub successful_ids: Vec<String>,
    pub allow_failed: bool,
    pub pending_failed: Vec<String>,
    pub pending_success: Vec<String>,
}

impl NormalResponseHandler {
    pub fn new(allow_failed: bool) -> NormalResponseHandler {
        NormalResponseHandler {
            failed_ids: vec![],
            successful_ids: vec![],
            pending_failed: vec![],
            pending_success: vec![],
            allow_failed,
        }
    }
}

impl PostResponseHandler for NormalResponseHandler {
    fn handle_response(&mut self, r: PostResponse, mid_batch: bool) -> error::Result<()> {
        if !r.is_success() {
            log::warn!("Got failure status from server while posting: {}", r.status);
            if r.status == status_codes::PRECONDITION_FAILED {
                return Err(ErrorKind::BatchInterrupted.into());
            } else {
                return Err(ErrorKind::StorageHttpError {
                    code: r.status,
                    route: "collection storage (TODO: record route somewhere)".into(),
                }
                .into());
            }
        }
        if !r.result.failed.is_empty() && !self.allow_failed {
            return Err(ErrorKind::RecordUploadFailed.into());
        }
        for id in r.result.success.iter() {
            self.pending_success.push(id.clone());
        }
        for kv in r.result.failed.iter() {
            self.pending_failed.push(kv.0.clone());
        }
        if !mid_batch {
            self.successful_ids.append(&mut self.pending_success);
            self.failed_ids.append(&mut self.pending_failed);
        }
        Ok(())
    }
}

impl<Poster, OnResponse> PostQueue<Poster, OnResponse>
where
    Poster: BatchPoster,
    OnResponse: PostResponseHandler,
{
    pub fn new(
        config: &InfoConfiguration,
        ts: ServerTimestamp,
        poster: Poster,
        on_response: OnResponse,
    ) -> PostQueue<Poster, OnResponse> {
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
            BatchState::Unsupported | BatchState::NoBatch => false,
            _ => true,
        }
    }

    pub fn enqueue(&mut self, record: &EncryptedBso) -> Result<bool> {
        let payload_length = record.payload.serialized_len();

        if self.post_limits.can_never_add(payload_length)
            || self.batch_limits.can_never_add(payload_length)
            || payload_length >= self.max_payload_bytes
        {
            log::warn!(
                "Single record too large to submit to server ({} b)",
                payload_length
            );
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

        debug_assert!(
            item_end >= payload_length,
            "EncryptedPayload::serialized_len is bugged"
        );

        // The + 1 is only relevant for the final record, which will have a trailing ']'.
        let item_len = item_end - item_start + 1;

        if item_len >= self.max_request_bytes {
            self.queued.truncate(item_start);
            log::warn!(
                "Single record too large to submit to server ({} b)",
                item_len
            );
            return Ok(false);
        }

        let can_post_record = self.post_limits.can_add_record(payload_length);
        let can_batch_record = self.batch_limits.can_add_record(payload_length);
        let can_send_record = self.queued.len() < self.max_request_bytes;

        if !can_post_record || !can_send_record || !can_batch_record {
            log::debug!(
                "PostQueue flushing! (can_post = {}, can_send = {}, can_batch = {})",
                can_post_record,
                can_send_record,
                can_batch_record
            );
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
        if self.queued.is_empty() {
            assert!(
                !self.in_batch(),
                "Bug: Somehow we're in a batch but have no queued records"
            );
            // Nothing to do!
            return Ok(());
        }

        self.queued.push(b']');
        let batch_id = match &self.batch {
            // Not the first post and we know we have no batch semantics.
            BatchState::Unsupported => None,
            // First commit in possible batch
            BatchState::NoBatch => Some("true".into()),
            // In a batch and we have a batch id.
            BatchState::InBatch(ref s) => Some(s.clone()),
        };

        log::info!(
            "Posting {} records of {} bytes",
            self.post_limits.cur_records,
            self.queued.len()
        );

        let is_commit = want_commit && batch_id.is_some();
        // Weird syntax for calling a function object that is a property.
        let resp_or_error = self.poster.post(
            self.queued.clone(),
            self.last_modified,
            batch_id,
            is_commit,
            self,
        );

        self.queued.truncate(0);

        if want_commit || self.batch == BatchState::Unsupported {
            self.batch_limits.clear();
        }
        self.post_limits.clear();

        let resp = resp_or_error?;

        if !resp.is_success() {
            let code = resp.status;
            self.on_response.handle_response(resp, !want_commit)?;
            log::error!("Bug: expected OnResponse to have bailed out!");
            // Should we assert here instead?
            return Err(ErrorKind::StorageHttpError {
                code,
                route: "Client bug!".into(),
            }
            .into());
        }

        if want_commit || self.batch == BatchState::Unsupported {
            self.last_modified = resp.last_modified;
        }

        if want_commit {
            log::debug!("Committed batch {:?}", self.batch);
            self.batch = BatchState::NoBatch;
            self.on_response.handle_response(resp, false)?;
            return Ok(());
        }

        if resp.status != status_codes::ACCEPTED {
            if self.in_batch() {
                return Err(ErrorKind::ServerBatchProblem(
                    "Server responded non-202 success code while a batch was in progress",
                )
                .into());
            }
            self.last_modified = resp.last_modified;
            self.batch = BatchState::Unsupported;
            self.batch_limits.clear();
            self.on_response.handle_response(resp, false)?;
            return Ok(());
        }

        let batch_id = resp
            .result
            .batch
            .as_ref()
            .ok_or_else(|| {
                ErrorKind::ServerBatchProblem("Invalid server response: 202 without a batch ID")
            })?
            .clone();

        match &self.batch {
            BatchState::Unsupported => {
                log::warn!("Server changed it's mind about supporting batching mid-batch...");
            }

            BatchState::InBatch(ref cur_id) => {
                if cur_id != &batch_id {
                    return Err(ErrorKind::ServerBatchProblem(
                        "Invalid server response: 202 without a batch ID",
                    )
                    .into());
                }
            }
            _ => {}
        }

        // Can't change this in match arms without NLL
        self.batch = BatchState::InBatch(batch_id);
        self.last_modified = resp.last_modified;

        self.on_response.handle_response(resp, true)?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct UploadInfo {
    pub successful_ids: Vec<String>,
    pub failed_ids: Vec<String>,
    pub modified_timestamp: ServerTimestamp,
}

impl<Poster> PostQueue<Poster, NormalResponseHandler> {
    // TODO: should take by move
    pub fn completed_upload_info(&mut self) -> UploadInfo {
        let mut result = UploadInfo {
            successful_ids: Vec::with_capacity(self.on_response.successful_ids.len()),
            failed_ids: Vec::with_capacity(
                self.on_response.failed_ids.len()
                    + self.on_response.pending_failed.len()
                    + self.on_response.pending_success.len(),
            ),
            modified_timestamp: self.last_modified,
        };

        result
            .successful_ids
            .append(&mut self.on_response.successful_ids);

        result.failed_ids.append(&mut self.on_response.failed_ids);
        result
            .failed_ids
            .append(&mut self.on_response.pending_failed);
        result
            .failed_ids
            .append(&mut self.on_response.pending_success);

        result
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::bso_record::{BsoRecord, EncryptedPayload};
    use lazy_static::lazy_static;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    #[test]
    fn test_url_building() {
        let base = Url::parse("https://example.com/sync").unwrap();
        let empty = CollectionRequest::new("foo")
            .build_url(base.clone())
            .unwrap();
        assert_eq!(empty.as_str(), "https://example.com/sync/storage/foo");
        let batch_start = CollectionRequest::new("bar")
            .batch(Some("true".into()))
            .commit(false)
            .build_url(base.clone())
            .unwrap();
        assert_eq!(
            batch_start.as_str(),
            "https://example.com/sync/storage/bar?batch=true"
        );
        let batch_commit = CollectionRequest::new("asdf")
            .batch(Some("1234abc".into()))
            .commit(true)
            .build_url(base.clone())
            .unwrap();
        assert_eq!(
            batch_commit.as_str(),
            "https://example.com/sync/storage/asdf?batch=1234abc&commit=true"
        );

        let idreq = CollectionRequest::new("wutang")
            .full()
            .ids(vec!["rza".into(), "gza".into()])
            .build_url(base.clone())
            .unwrap();
        assert_eq!(
            idreq.as_str(),
            "https://example.com/sync/storage/wutang?full=1&ids=rza%2Cgza"
        );

        let complex = CollectionRequest::new("specific")
            .full()
            .limit(10)
            .sort_by(RequestOrder::Oldest)
            .older_than(ServerTimestamp(9876.54))
            .newer_than(ServerTimestamp(1234.56))
            .build_url(base.clone())
            .unwrap();
        assert_eq!(complex.as_str(),
            "https://example.com/sync/storage/specific?full=1&limit=10&older=9876.54&newer=1234.56&sort=oldest");
    }

    #[derive(Debug, Clone)]
    struct PostedData {
        body: String,
        xius: ServerTimestamp,
        batch: Option<String>,
        commit: bool,
        payload_bytes: usize,
        records: usize,
    }

    impl PostedData {
        fn records_as_json(&self) -> Vec<serde_json::Value> {
            let values =
                serde_json::from_str::<serde_json::Value>(&self.body).expect("Posted invalid json");
            // Check that they actually deserialize as what we want
            let records_or_err = serde_json::from_value::<Vec<EncryptedBso>>(values.clone());
            records_or_err.expect("Failed to deserialize data");
            serde_json::from_value(values).unwrap()
        }
    }

    #[derive(Debug, Clone)]
    struct BatchInfo {
        id: Option<String>,
        posts: Vec<PostedData>,
        bytes: usize,
        records: usize,
    }

    #[derive(Debug, Clone)]
    struct TestPoster {
        all_posts: Vec<PostedData>,
        responses: VecDeque<PostResponse>,
        batches: Vec<BatchInfo>,
        cur_batch: Option<BatchInfo>,
        cfg: InfoConfiguration,
    }

    type TestPosterRef = Rc<RefCell<TestPoster>>;
    impl TestPoster {
        pub fn new<T>(cfg: &InfoConfiguration, responses: T) -> TestPosterRef
        where
            T: Into<VecDeque<PostResponse>>,
        {
            Rc::new(RefCell::new(TestPoster {
                all_posts: vec![],
                responses: responses.into(),
                batches: vec![],
                cur_batch: None,
                cfg: cfg.clone(),
            }))
        }
        // Adds &mut
        fn do_post<T, O>(
            &mut self,
            body: &[u8],
            xius: ServerTimestamp,
            batch: Option<String>,
            commit: bool,
            queue: &PostQueue<T, O>,
        ) -> Result<PostResponse> {
            let mut post = PostedData {
                body: String::from_utf8(body.into()).expect("Posted invalid utf8..."),
                batch: batch.clone(),
                xius,
                commit,
                payload_bytes: 0,
                records: 0,
            };

            assert!(body.len() <= self.cfg.max_request_bytes);

            let (num_records, record_payload_bytes) = {
                let recs = post.records_as_json();
                assert!(recs.len() <= self.cfg.max_post_records);
                assert!(recs.len() <= self.cfg.max_total_records);
                let payload_bytes: usize = recs
                    .iter()
                    .map(|r| {
                        let len = r["payload"]
                            .as_str()
                            .expect("Non string payload property")
                            .len();
                        assert!(len <= self.cfg.max_record_payload_bytes);
                        len
                    })
                    .sum();
                assert!(payload_bytes <= self.cfg.max_post_bytes);
                assert!(payload_bytes <= self.cfg.max_total_bytes);

                assert_eq!(queue.post_limits.cur_bytes, payload_bytes);
                assert_eq!(queue.post_limits.cur_records, recs.len());
                (recs.len(), payload_bytes)
            };
            post.payload_bytes = record_payload_bytes;
            post.records = num_records;

            self.all_posts.push(post.clone());
            let response = self.responses.pop_front().unwrap();

            if self.cur_batch.is_none() {
                assert!(
                    batch.is_none() || batch == Some("true".into()),
                    "We shouldn't be in a batch now"
                );
                self.cur_batch = Some(BatchInfo {
                    id: response.result.batch.clone(),
                    posts: vec![],
                    records: 0,
                    bytes: 0,
                });
            } else {
                assert_eq!(
                    batch,
                    self.cur_batch.as_ref().unwrap().id,
                    "We're in a batch but got the wrong batch id"
                );
            }

            {
                let batch = self.cur_batch.as_mut().unwrap();
                batch.posts.push(post.clone());
                batch.records += num_records;
                batch.bytes += record_payload_bytes;

                assert!(batch.bytes <= self.cfg.max_total_bytes);
                assert!(batch.records <= self.cfg.max_total_records);

                assert_eq!(batch.records, queue.batch_limits.cur_records);
                assert_eq!(batch.bytes, queue.batch_limits.cur_bytes);
            }

            if commit || response.result.batch.is_none() {
                let batch = self.cur_batch.take().unwrap();
                self.batches.push(batch);
            }

            Ok(response)
        }

        fn do_handle_response(&mut self, _: PostResponse, mid_batch: bool) -> Result<()> {
            assert_eq!(mid_batch, self.cur_batch.is_some());
            Ok(())
        }
    }
    impl BatchPoster for TestPosterRef {
        fn post<T, O>(
            &self,
            body: Vec<u8>,
            xius: ServerTimestamp,
            batch: Option<String>,
            commit: bool,
            queue: &PostQueue<T, O>,
        ) -> Result<PostResponse> {
            self.borrow_mut().do_post(&body, xius, batch, commit, queue)
        }
    }

    impl PostResponseHandler for TestPosterRef {
        fn handle_response(&mut self, r: PostResponse, mid_batch: bool) -> Result<()> {
            self.borrow_mut().do_handle_response(r, mid_batch)
        }
    }

    type MockedPostQueue = PostQueue<TestPosterRef, TestPosterRef>;

    fn pq_test_setup(
        cfg: InfoConfiguration,
        lm: f64,
        resps: Vec<PostResponse>,
    ) -> (MockedPostQueue, TestPosterRef) {
        let tester = TestPoster::new(&cfg, resps);
        let pq = PostQueue::new(&cfg, ServerTimestamp(lm), tester.clone(), tester.clone());
        (pq, tester)
    }

    fn fake_response<'a, T: Into<Option<&'a str>>>(status: u16, lm: f64, batch: T) -> PostResponse {
        PostResponse {
            status,
            last_modified: ServerTimestamp(lm),
            result: UploadResult {
                batch: batch.into().map(Into::into),
                failed: HashMap::new(),
                success: vec![],
            },
        }
    }

    lazy_static! {
        // ~40b
        static ref PAYLOAD_OVERHEAD: usize = {
            let payload = EncryptedPayload {
                iv: "".into(),
                hmac: "".into(),
                ciphertext: "".into()
            };
            serde_json::to_string(&payload).unwrap().len()
        };
        // ~80b
        static ref TOTAL_RECORD_OVERHEAD: usize = {
            let val = serde_json::to_value(BsoRecord {
                id: "".into(),
                collection: "".into(),
                modified: ServerTimestamp(0.0),
                sortindex: None,
                ttl: None,
                payload: EncryptedPayload {
                    iv: "".into(),
                    hmac: "".into(),
                    ciphertext: "".into()
                },
            }).unwrap();
            serde_json::to_string(&val).unwrap().len()
        };
        // There's some subtlety in how we calulate this having to do with the fact that
        // the quotes in the payload are escaped but the escape chars count to the request len
        // and *not* to the payload len (the payload len check happens after json parsing the
        // top level object).
        static ref NON_PAYLOAD_OVERHEAD: usize = {
            *TOTAL_RECORD_OVERHEAD - *PAYLOAD_OVERHEAD
        };
    }

    // Actual record size (for max_request_len) will be larger by some amount
    fn make_record(payload_size: usize) -> EncryptedBso {
        assert!(payload_size > *PAYLOAD_OVERHEAD);
        let ciphertext_len = payload_size - *PAYLOAD_OVERHEAD;
        BsoRecord {
            id: "".into(),
            collection: "".into(),
            modified: ServerTimestamp(0.0),
            sortindex: None,
            ttl: None,
            payload: EncryptedPayload {
                iv: "".into(),
                hmac: "".into(),
                ciphertext: "x".repeat(ciphertext_len),
            },
        }
    }

    fn request_bytes_for_payloads(payloads: &[usize]) -> usize {
        1 + payloads
            .iter()
            .map(|&size| size + 1 + *NON_PAYLOAD_OVERHEAD)
            .sum::<usize>()
    }

    #[test]
    fn test_pq_basic() {
        let cfg = InfoConfiguration {
            max_request_bytes: 1000,
            max_record_payload_bytes: 1000,
            ..InfoConfiguration::default()
        };
        let time = 11_111_111.0;
        let (mut pq, tester) = pq_test_setup(
            cfg,
            time,
            vec![fake_response(status_codes::OK, time + 100.0, None)],
        );

        pq.enqueue(&make_record(100)).unwrap();
        pq.flush(true).unwrap();

        let t = tester.borrow();
        assert!(t.cur_batch.is_none());
        assert_eq!(t.all_posts.len(), 1);
        assert_eq!(t.batches.len(), 1);
        assert_eq!(t.batches[0].posts.len(), 1);
        assert_eq!(t.batches[0].records, 1);
        assert_eq!(t.batches[0].bytes, 100);
        assert_eq!(
            t.batches[0].posts[0].body.len(),
            request_bytes_for_payloads(&[100])
        );
    }

    #[test]
    fn test_pq_max_request_bytes_no_batch() {
        let cfg = InfoConfiguration {
            max_request_bytes: 250,
            ..InfoConfiguration::default()
        };
        let time = 11_111_111.0;
        let (mut pq, tester) = pq_test_setup(
            cfg,
            time,
            vec![
                fake_response(status_codes::OK, time + 100.0, None),
                fake_response(status_codes::OK, time + 200.0, None),
            ],
        );

        // Note that the total record overhead is around 85 bytes
        let payload_size = 100 - *NON_PAYLOAD_OVERHEAD;
        pq.enqueue(&make_record(payload_size)).unwrap(); // total size == 102; [r]
        pq.enqueue(&make_record(payload_size)).unwrap(); // total size == 203; [r,r]
        pq.enqueue(&make_record(payload_size)).unwrap(); // too big, 2nd post.
        pq.flush(true).unwrap();

        let t = tester.borrow();
        assert!(t.cur_batch.is_none());
        assert_eq!(t.all_posts.len(), 2);
        assert_eq!(t.batches.len(), 2);
        assert_eq!(t.batches[0].posts.len(), 1);
        assert_eq!(t.batches[0].records, 2);
        assert_eq!(t.batches[0].bytes, payload_size * 2);
        assert_eq!(t.batches[0].posts[0].batch, Some("true".into()));
        assert_eq!(
            t.batches[0].posts[0].body.len(),
            request_bytes_for_payloads(&[payload_size, payload_size])
        );

        assert_eq!(t.batches[1].posts.len(), 1);
        assert_eq!(t.batches[1].records, 1);
        assert_eq!(t.batches[1].bytes, payload_size);
        // We know at this point that the server does not support batching.
        assert_eq!(t.batches[1].posts[0].batch, None);
        assert_eq!(t.batches[1].posts[0].commit, false);
        assert_eq!(
            t.batches[1].posts[0].body.len(),
            request_bytes_for_payloads(&[payload_size])
        );
    }

    #[test]
    fn test_pq_max_record_payload_bytes_no_batch() {
        let cfg = InfoConfiguration {
            max_record_payload_bytes: 150,
            max_request_bytes: 350,
            ..InfoConfiguration::default()
        };
        let time = 11_111_111.0;
        let (mut pq, tester) = pq_test_setup(
            cfg,
            time,
            vec![
                fake_response(status_codes::OK, time + 100.0, None),
                fake_response(status_codes::OK, time + 200.0, None),
            ],
        );

        // Note that the total record overhead is around 85 bytes
        let payload_size = 100 - *NON_PAYLOAD_OVERHEAD;
        pq.enqueue(&make_record(payload_size)).unwrap(); // total size == 102; [r]
        let enqueued = pq.enqueue(&make_record(151)).unwrap(); // still 102
        assert!(!enqueued, "Should not have fit");
        pq.enqueue(&make_record(payload_size)).unwrap();
        pq.flush(true).unwrap();

        let t = tester.borrow();
        assert!(t.cur_batch.is_none());
        assert_eq!(t.all_posts.len(), 1);
        assert_eq!(t.batches.len(), 1);
        assert_eq!(t.batches[0].posts.len(), 1);
        assert_eq!(t.batches[0].records, 2);
        assert_eq!(t.batches[0].bytes, payload_size * 2);
        assert_eq!(
            t.batches[0].posts[0].body.len(),
            request_bytes_for_payloads(&[payload_size, payload_size])
        );
    }

    #[test]
    fn test_pq_single_batch() {
        let cfg = InfoConfiguration::default();
        let time = 11_111_111.0;
        let (mut pq, tester) = pq_test_setup(
            cfg,
            time,
            vec![fake_response(
                status_codes::ACCEPTED,
                time + 100.0,
                Some("1234"),
            )],
        );

        let payload_size = 100 - *NON_PAYLOAD_OVERHEAD;
        pq.enqueue(&make_record(payload_size)).unwrap();
        pq.enqueue(&make_record(payload_size)).unwrap();
        pq.enqueue(&make_record(payload_size)).unwrap();
        pq.flush(true).unwrap();

        let t = tester.borrow();
        assert!(t.cur_batch.is_none());
        assert_eq!(t.all_posts.len(), 1);
        assert_eq!(t.batches.len(), 1);
        assert_eq!(t.batches[0].id.as_ref().unwrap(), "1234");
        assert_eq!(t.batches[0].posts.len(), 1);
        assert_eq!(t.batches[0].records, 3);
        assert_eq!(t.batches[0].bytes, payload_size * 3);
        assert_eq!(t.batches[0].posts[0].commit, true);
        assert_eq!(
            t.batches[0].posts[0].body.len(),
            request_bytes_for_payloads(&[payload_size, payload_size, payload_size])
        );
    }

    #[test]
    fn test_pq_multi_post_batch_bytes() {
        let cfg = InfoConfiguration {
            max_post_bytes: 200,
            ..InfoConfiguration::default()
        };
        let time = 11_111_111.0;
        let (mut pq, tester) = pq_test_setup(
            cfg,
            time,
            vec![
                fake_response(status_codes::ACCEPTED, time, Some("1234")),
                fake_response(status_codes::ACCEPTED, time + 100.0, Some("1234")),
            ],
        );

        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        // POST
        pq.enqueue(&make_record(100)).unwrap();
        pq.flush(true).unwrap(); // COMMIT

        let t = tester.borrow();
        assert!(t.cur_batch.is_none());
        assert_eq!(t.all_posts.len(), 2);
        assert_eq!(t.batches.len(), 1);
        assert_eq!(t.batches[0].posts.len(), 2);
        assert_eq!(t.batches[0].records, 3);
        assert_eq!(t.batches[0].bytes, 300);

        assert_eq!(t.batches[0].posts[0].batch.as_ref().unwrap(), "true");
        assert_eq!(t.batches[0].posts[0].records, 2);
        assert_eq!(t.batches[0].posts[0].payload_bytes, 200);
        assert_eq!(t.batches[0].posts[0].commit, false);
        assert_eq!(
            t.batches[0].posts[0].body.len(),
            request_bytes_for_payloads(&[100, 100])
        );

        assert_eq!(t.batches[0].posts[1].batch.as_ref().unwrap(), "1234");
        assert_eq!(t.batches[0].posts[1].records, 1);
        assert_eq!(t.batches[0].posts[1].payload_bytes, 100);
        assert_eq!(t.batches[0].posts[1].commit, true);
        assert_eq!(
            t.batches[0].posts[1].body.len(),
            request_bytes_for_payloads(&[100])
        );
    }

    #[test]
    fn test_pq_multi_post_batch_records() {
        let cfg = InfoConfiguration {
            max_post_records: 3,
            ..InfoConfiguration::default()
        };
        let time = 11_111_111.0;
        let (mut pq, tester) = pq_test_setup(
            cfg,
            time,
            vec![
                fake_response(status_codes::ACCEPTED, time, Some("1234")),
                fake_response(status_codes::ACCEPTED, time, Some("1234")),
                fake_response(status_codes::ACCEPTED, time + 100.0, Some("1234")),
            ],
        );

        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        // POST
        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        // POST
        pq.enqueue(&make_record(100)).unwrap();
        pq.flush(true).unwrap(); // COMMIT

        let t = tester.borrow();
        assert!(t.cur_batch.is_none());
        assert_eq!(t.all_posts.len(), 3);
        assert_eq!(t.batches.len(), 1);
        assert_eq!(t.batches[0].posts.len(), 3);
        assert_eq!(t.batches[0].records, 7);
        assert_eq!(t.batches[0].bytes, 700);

        assert_eq!(t.batches[0].posts[0].batch.as_ref().unwrap(), "true");
        assert_eq!(t.batches[0].posts[0].records, 3);
        assert_eq!(t.batches[0].posts[0].payload_bytes, 300);
        assert_eq!(t.batches[0].posts[0].commit, false);
        assert_eq!(
            t.batches[0].posts[0].body.len(),
            request_bytes_for_payloads(&[100, 100, 100])
        );

        assert_eq!(t.batches[0].posts[1].batch.as_ref().unwrap(), "1234");
        assert_eq!(t.batches[0].posts[1].records, 3);
        assert_eq!(t.batches[0].posts[1].payload_bytes, 300);
        assert_eq!(t.batches[0].posts[1].commit, false);
        assert_eq!(
            t.batches[0].posts[1].body.len(),
            request_bytes_for_payloads(&[100, 100, 100])
        );

        assert_eq!(t.batches[0].posts[2].batch.as_ref().unwrap(), "1234");
        assert_eq!(t.batches[0].posts[2].records, 1);
        assert_eq!(t.batches[0].posts[2].payload_bytes, 100);
        assert_eq!(t.batches[0].posts[2].commit, true);
        assert_eq!(
            t.batches[0].posts[2].body.len(),
            request_bytes_for_payloads(&[100])
        );
    }

    #[test]
    #[allow(clippy::cyclomatic_complexity)]
    fn test_pq_multi_post_multi_batch_records() {
        let cfg = InfoConfiguration {
            max_post_records: 3,
            max_total_records: 5,
            ..InfoConfiguration::default()
        };
        let time = 11_111_111.0;
        let (mut pq, tester) = pq_test_setup(
            cfg,
            time,
            vec![
                fake_response(status_codes::ACCEPTED, time, Some("1234")),
                fake_response(status_codes::ACCEPTED, time + 100.0, Some("1234")),
                fake_response(status_codes::ACCEPTED, time + 100.0, Some("abcd")),
                fake_response(status_codes::ACCEPTED, time + 200.0, Some("abcd")),
            ],
        );

        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        // POST
        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        // POST + COMMIT
        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        // POST
        pq.enqueue(&make_record(100)).unwrap();
        pq.flush(true).unwrap(); // COMMIT

        let t = tester.borrow();
        assert!(t.cur_batch.is_none());
        assert_eq!(t.all_posts.len(), 4);
        assert_eq!(t.batches.len(), 2);
        assert_eq!(t.batches[0].posts.len(), 2);
        assert_eq!(t.batches[1].posts.len(), 2);

        assert_eq!(t.batches[0].records, 5);
        assert_eq!(t.batches[1].records, 4);

        assert_eq!(t.batches[0].bytes, 500);
        assert_eq!(t.batches[1].bytes, 400);

        assert_eq!(t.batches[0].posts[0].batch.as_ref().unwrap(), "true");
        assert_eq!(t.batches[0].posts[0].records, 3);
        assert_eq!(t.batches[0].posts[0].payload_bytes, 300);
        assert_eq!(t.batches[0].posts[0].commit, false);
        assert_eq!(
            t.batches[0].posts[0].body.len(),
            request_bytes_for_payloads(&[100, 100, 100])
        );

        assert_eq!(t.batches[0].posts[1].batch.as_ref().unwrap(), "1234");
        assert_eq!(t.batches[0].posts[1].records, 2);
        assert_eq!(t.batches[0].posts[1].payload_bytes, 200);
        assert_eq!(t.batches[0].posts[1].commit, true);
        assert_eq!(
            t.batches[0].posts[1].body.len(),
            request_bytes_for_payloads(&[100, 100])
        );

        assert_eq!(t.batches[1].posts[0].batch.as_ref().unwrap(), "true");
        assert_eq!(t.batches[1].posts[0].records, 3);
        assert_eq!(t.batches[1].posts[0].payload_bytes, 300);
        assert_eq!(t.batches[1].posts[0].commit, false);
        assert_eq!(
            t.batches[1].posts[0].body.len(),
            request_bytes_for_payloads(&[100, 100, 100])
        );

        assert_eq!(t.batches[1].posts[1].batch.as_ref().unwrap(), "abcd");
        assert_eq!(t.batches[1].posts[1].records, 1);
        assert_eq!(t.batches[1].posts[1].payload_bytes, 100);
        assert_eq!(t.batches[1].posts[1].commit, true);
        assert_eq!(
            t.batches[1].posts[1].body.len(),
            request_bytes_for_payloads(&[100])
        );
    }

    macro_rules! assert_feq {
        ($a:expr, $b:expr) => {
            let a = $a;
            let b = $b;
            assert!(
                (a - b).abs() < std::f64::EPSILON,
                "assert_feq failure: {} != {}",
                a,
                b
            )
        };
    }

    #[test]
    #[allow(clippy::cyclomatic_complexity)]
    fn test_pq_multi_post_multi_batch_bytes() {
        let cfg = InfoConfiguration {
            max_post_bytes: 300,
            max_total_bytes: 500,
            ..InfoConfiguration::default()
        };
        let time = 11_111_111.0;
        let (mut pq, tester) = pq_test_setup(
            cfg,
            time,
            vec![
                fake_response(status_codes::ACCEPTED, time, Some("1234")),
                fake_response(status_codes::ACCEPTED, time + 100.0, Some("1234")), // should commit
                fake_response(status_codes::ACCEPTED, time + 100.0, Some("abcd")),
                fake_response(status_codes::ACCEPTED, time + 200.0, Some("abcd")), // should commit
            ],
        );

        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        assert_feq!(pq.last_modified.0, time);
        // POST
        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();
        // POST + COMMIT
        pq.enqueue(&make_record(100)).unwrap();
        assert_feq!(pq.last_modified.0, time + 100.0);
        pq.enqueue(&make_record(100)).unwrap();
        pq.enqueue(&make_record(100)).unwrap();

        // POST
        pq.enqueue(&make_record(100)).unwrap();
        assert_feq!(pq.last_modified.0, time + 100.0);
        pq.flush(true).unwrap(); // COMMIT

        assert_feq!(pq.last_modified.0, time + 200.0);

        let t = tester.borrow();
        assert!(t.cur_batch.is_none());
        assert_eq!(t.all_posts.len(), 4);
        assert_eq!(t.batches.len(), 2);
        assert_eq!(t.batches[0].posts.len(), 2);
        assert_eq!(t.batches[1].posts.len(), 2);

        assert_eq!(t.batches[0].records, 5);
        assert_eq!(t.batches[1].records, 4);

        assert_eq!(t.batches[0].bytes, 500);
        assert_eq!(t.batches[1].bytes, 400);

        assert_eq!(t.batches[0].posts[0].batch.as_ref().unwrap(), "true");
        assert_eq!(t.batches[0].posts[0].records, 3);
        assert_eq!(t.batches[0].posts[0].payload_bytes, 300);
        assert_eq!(t.batches[0].posts[0].commit, false);
        assert_eq!(
            t.batches[0].posts[0].body.len(),
            request_bytes_for_payloads(&[100, 100, 100])
        );

        assert_eq!(t.batches[0].posts[1].batch.as_ref().unwrap(), "1234");
        assert_eq!(t.batches[0].posts[1].records, 2);
        assert_eq!(t.batches[0].posts[1].payload_bytes, 200);
        assert_eq!(t.batches[0].posts[1].commit, true);
        assert_eq!(
            t.batches[0].posts[1].body.len(),
            request_bytes_for_payloads(&[100, 100])
        );

        assert_eq!(t.batches[1].posts[0].batch.as_ref().unwrap(), "true");
        assert_eq!(t.batches[1].posts[0].records, 3);
        assert_eq!(t.batches[1].posts[0].payload_bytes, 300);
        assert_eq!(t.batches[1].posts[0].commit, false);
        assert_eq!(
            t.batches[1].posts[0].body.len(),
            request_bytes_for_payloads(&[100, 100, 100])
        );

        assert_eq!(t.batches[1].posts[1].batch.as_ref().unwrap(), "abcd");
        assert_eq!(t.batches[1].posts[1].records, 1);
        assert_eq!(t.batches[1].posts[1].payload_bytes, 100);
        assert_eq!(t.batches[1].posts[1].commit, true);
        assert_eq!(
            t.batches[1].posts[1].body.len(),
            request_bytes_for_payloads(&[100])
        );
    }

    // TODO: Test
    //
    // - error cases!!! We don't test our handling of server errors at all!
    // - mixed bytes/record limits
    //
    // A lot of these have good examples in test_postqueue.js on deskftop sync

}

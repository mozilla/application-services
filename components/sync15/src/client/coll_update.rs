/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{
    CollState, Sync15ClientResponse, Sync15StorageClient,
    request::{NormalResponseHandler, UploadInfo},
};
use crate::bso::{IncomingBso, IncomingEncryptedBso, OutgoingBso, OutgoingEncryptedBso};
use crate::engine::{CollectionRequest, RequestOrder};
use crate::error::{self, Error, ErrorResponse, Result, trace};
use crate::{CollectionName, KeyBundle, ServerTimestamp};

fn encrypt_outgoing(o: Vec<OutgoingBso>, key: &KeyBundle) -> Result<Vec<OutgoingEncryptedBso>> {
    o.into_iter()
        .map(|change| change.into_encrypted(key))
        .collect()
}

pub fn fetch_incoming(
    client: &Sync15StorageClient,
    state: &CollState,
    collection_request: CollectionRequest,
) -> Result<Vec<IncomingBso>> {
    let (records, _timestamp) = match client.get_encrypted_records(collection_request)? {
        Sync15ClientResponse::Success {
            record,
            last_modified,
            ..
        } => (record, last_modified),
        other => return Err(other.create_storage_error()),
    };
    let mut result = Vec::with_capacity(records.len());
    for record in records {
        // if we see a HMAC error, we've made an explicit decision to
        // NOT handle it here, but restart the global state machine.
        // That should cause us to re-read crypto/keys and things should
        // work (although if for some reason crypto/keys was updated but
        // not all storage was wiped we are probably screwed.)
        result.push(record.into_decrypted(&state.key)?);
    }
    Ok(result)
}

/// How many records we request per page when downloading a collection.
/// Matches desktop's `DEFAULT_DOWNLOAD_BATCH_SIZE`.
// TODO: let engines specify this via a new `SyncEngine` method.
pub const DEFAULT_DOWNLOAD_BATCH_SIZE: usize = 1000;

/// What a single page fetch returns: the response, plus `X-Weave-Next-Offset`.
/// Matches `Sync15StorageClient::get_encrypted_records_page`.
type IncomingPage = (
    Sync15ClientResponse<Vec<IncomingEncryptedBso>>,
    Option<String>,
);

/// Pages through a collection `batch_size` records at a time, decrypting each
/// page and passing it to `on_batch`. This is the equivalent of desktop's
/// `Collection.getBatched()`, except that pages are handed over as they arrive
/// instead of being held in memory.
///
/// - If `collection_request` already has a limit (eg, history asks for the newest
///   5000), that's the total across all pages and its sort order is kept.
///   Otherwise we sort by `oldest` so offsets are stable.
/// - Every page after the first is sent with `X-If-Unmodified-Since` set to the
///   first page's `X-Last-Modified`. A 412 (or a changed timestamp) on a later
///   page returns `Error::CollectionChangedDuringDownload`. Earlier pages will
///   already have been passed to `on_batch`, so callers must not advance any
///   timestamps unless this returns `Ok`.
/// - `on_batch` is always called at least once, even if the collection is empty,
///   because some engines (eg, tabs) do housekeeping in `stage_incoming`.
///
/// `fetch_page` is a closure so tests can supply fake pages; in production it's
/// `|req, xius| client.get_encrypted_records_page(req, xius)`.
///
/// Returns the total number of records fetched.
pub fn fetch_incoming_batched(
    key: &KeyBundle,
    collection_request: CollectionRequest,
    batch_size: usize,
    mut fetch_page: impl FnMut(CollectionRequest, Option<ServerTimestamp>) -> Result<IncomingPage>,
    mut on_batch: impl FnMut(Vec<IncomingBso>) -> Result<()>,
) -> Result<usize> {
    assert!(batch_size > 0, "batch_size must be greater than zero");
    let (total_limit, order) = match collection_request.limit {
        Some(l) => (l.num, l.order),
        None => (usize::MAX, RequestOrder::Oldest),
    };

    let mut fetched = 0;
    let mut offset: Option<String> = None;
    // `None` until we've seen the first page; then it's both the value we send
    // as X-If-Unmodified-Since and the value later pages must match.
    let mut batch_last_modified: Option<ServerTimestamp> = None;
    loop {
        let page_limit = batch_size.min(total_limit - fetched);
        let request = collection_request
            .clone()
            .limit(page_limit, order)
            .offset(offset.take());
        trace!(
            "Performing batched GET (limit={}, offset={:?})",
            page_limit, request.offset
        );

        let (resp, next_offset) = fetch_page(request, batch_last_modified)?;
        let (records, last_modified) = match resp {
            Sync15ClientResponse::Success {
                record,
                last_modified,
                ..
            } => (record, last_modified),
            // A 412 can only happen on later pages, since that's the only time
            // we send X-If-Unmodified-Since.
            Sync15ClientResponse::Error(ErrorResponse::PreconditionFailed { .. })
                if batch_last_modified.is_some() =>
            {
                return Err(Error::CollectionChangedDuringDownload);
            }
            other => return Err(other.create_storage_error()),
        };

        match batch_last_modified {
            None => batch_last_modified = Some(last_modified),
            // Should be impossible - the server should have 412'd instead.
            Some(expected) if expected != last_modified => {
                return Err(Error::CollectionChangedDuringDownload);
            }
            Some(_) => {}
        }

        let page_len = records.len();
        fetched += page_len;
        let decrypted = records
            .into_iter()
            .map(|record| record.into_decrypted(key))
            .collect::<Result<Vec<_>>>()?;
        on_batch(decrypted)?;

        match next_offset {
            // An empty page with an offset would be a server bug; don't spin on it.
            Some(next) if fetched < total_limit && page_len > 0 => offset = Some(next),
            _ => break,
        }
    }
    Ok(fetched)
}

pub struct CollectionUpdate<'a> {
    client: &'a Sync15StorageClient,
    state: &'a CollState,
    collection: CollectionName,
    xius: ServerTimestamp,
    to_update: Vec<OutgoingEncryptedBso>,
    fully_atomic: bool,
}

impl<'a> CollectionUpdate<'a> {
    pub fn new(
        client: &'a Sync15StorageClient,
        state: &'a CollState,
        collection: CollectionName,
        xius: ServerTimestamp,
        records: Vec<OutgoingEncryptedBso>,
        fully_atomic: bool,
    ) -> CollectionUpdate<'a> {
        CollectionUpdate {
            client,
            state,
            collection,
            xius,
            to_update: records,
            fully_atomic,
        }
    }

    pub fn new_from_changeset(
        client: &'a Sync15StorageClient,
        state: &'a CollState,
        collection: CollectionName,
        changeset: Vec<OutgoingBso>,
        fully_atomic: bool,
    ) -> Result<CollectionUpdate<'a>> {
        let to_update = encrypt_outgoing(changeset, &state.key)?;
        Ok(CollectionUpdate::new(
            client,
            state,
            collection,
            state.last_modified,
            to_update,
            fully_atomic,
        ))
    }

    /// Returns a list of the IDs that failed if allowed_dropped_records is true, otherwise
    /// returns an empty vec.
    pub fn upload(self) -> error::Result<UploadInfo> {
        let mut failed = vec![];
        let mut q = self.client.new_post_queue(
            &self.collection,
            &self.state.config,
            self.xius,
            NormalResponseHandler::new(!self.fully_atomic),
        )?;

        for record in self.to_update.into_iter() {
            let enqueued = q.enqueue(&record)?;
            if !enqueued && self.fully_atomic {
                return Err(Error::RecordTooLargeError);
            }
        }

        q.flush(true)?;
        let mut info = q.completed_upload_info();
        info.failed_ids.append(&mut failed);
        if self.fully_atomic {
            assert_eq!(
                info.failed_ids.len(),
                0,
                "Bug: Should have failed by now if we aren't allowing dropped records"
            );
        }
        Ok(info)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::bso::IncomingEnvelope;
    use crate::{EncryptedPayload, Guid};

    const TS: i64 = 1_000_000;

    fn key() -> KeyBundle {
        nss_as::ensure_initialized();
        KeyBundle::new_random().unwrap()
    }

    /// A successful page with the given record ids, `X-Last-Modified` and
    /// `X-Weave-Next-Offset`.
    fn page(key: &KeyBundle, ids: &[&str], ts: i64, next: Option<&str>) -> Result<IncomingPage> {
        let records = ids
            .iter()
            .map(|id| {
                IncomingEncryptedBso::new(
                    IncomingEnvelope {
                        id: Guid::new(id),
                        modified: ServerTimestamp::from_millis(ts),
                        sortindex: None,
                        ttl: None,
                    },
                    EncryptedPayload::from_cleartext(
                        key,
                        serde_json::json!({ "id": id }).to_string(),
                    )
                    .unwrap(),
                )
            })
            .collect();
        Ok((
            Sync15ClientResponse::Success {
                status: 200,
                record: records,
                last_modified: ServerTimestamp::from_millis(ts),
                route: "storage/test".into(),
            },
            next.map(str::to_string),
        ))
    }

    fn error_page(e: ErrorResponse) -> Result<IncomingPage> {
        Ok((Sync15ClientResponse::Error(e), None))
    }

    fn precondition_failed() -> ErrorResponse {
        ErrorResponse::PreconditionFailed {
            route: "storage/test".into(),
        }
    }

    /// What a download did: every request made (with the X-If-Unmodified-Since
    /// sent), the ids in each batch handed to `on_batch`, and the result.
    struct Download {
        requests: Vec<(CollectionRequest, Option<ServerTimestamp>)>,
        batches: Vec<Vec<String>>,
        result: Result<usize>,
    }

    /// Runs the loop against `pages`, served in order. Panics if the loop asks
    /// for more pages than were supplied.
    fn download(
        key: &KeyBundle,
        request: CollectionRequest,
        batch_size: usize,
        pages: Vec<Result<IncomingPage>>,
    ) -> Download {
        let mut pages = pages.into_iter();
        let mut requests = Vec::new();
        let mut batches = Vec::new();
        let result = fetch_incoming_batched(
            key,
            request,
            batch_size,
            |req, xius| {
                requests.push((req, xius));
                pages
                    .next()
                    .expect("loop requested more pages than expected")
            },
            |batch| {
                batches.push(
                    batch
                        .into_iter()
                        .map(|b| b.envelope.id.to_string())
                        .collect(),
                );
                Ok(())
            },
        );
        Download {
            requests,
            batches,
            result,
        }
    }

    fn request() -> CollectionRequest {
        CollectionRequest::new("test".into())
            .full()
            .newer_than(ServerTimestamp::from_millis(500))
    }

    // `RequestLimit` isn't nameable outside `engine`, so compare its fields.
    fn limit_of(req: &CollectionRequest) -> Option<(usize, RequestOrder)> {
        req.limit.map(|l| (l.num, l.order))
    }

    fn limit(num: usize, order: RequestOrder) -> Option<(usize, RequestOrder)> {
        Some((num, order))
    }

    #[test]
    fn test_single_page() {
        let key = key();
        let d = download(
            &key,
            request(),
            1000,
            vec![page(&key, &["a", "b"], TS, None)],
        );

        assert_eq!(d.result.unwrap(), 2);
        assert_eq!(d.batches, vec![vec!["a", "b"]]);
        assert_eq!(d.requests.len(), 1);
        let (req, xius) = &d.requests[0];
        // No engine limit, so we page by `oldest`; nothing else changes.
        assert_eq!(limit_of(req), limit(1000, RequestOrder::Oldest));
        assert_eq!(req.offset, None);
        assert!(req.full);
        assert_eq!(req.newer, Some(ServerTimestamp::from_millis(500)));
        assert_eq!(
            *xius, None,
            "the first page must not send X-If-Unmodified-Since"
        );
    }

    #[test]
    fn test_multiple_pages() {
        let key = key();
        let d = download(
            &key,
            request(),
            2,
            vec![
                page(&key, &["a", "b"], TS, Some("off-1")),
                page(&key, &["c", "d"], TS, Some("off-2")),
                page(&key, &["e"], TS, None),
            ],
        );

        assert_eq!(d.result.unwrap(), 5);
        assert_eq!(d.batches, vec![vec!["a", "b"], vec!["c", "d"], vec!["e"]]);
        let offsets: Vec<_> = d.requests.iter().map(|(r, _)| r.offset.clone()).collect();
        assert_eq!(
            offsets,
            vec![None, Some("off-1".into()), Some("off-2".into())]
        );
        let xius: Vec<_> = d.requests.iter().map(|(_, x)| *x).collect();
        let pinned = Some(ServerTimestamp::from_millis(TS));
        assert_eq!(xius, vec![None, pinned, pinned]);
        for (req, _) in &d.requests {
            assert_eq!(limit_of(req), limit(2, RequestOrder::Oldest));
            assert!(req.full);
            assert_eq!(req.newer, Some(ServerTimestamp::from_millis(500)));
        }
    }

    #[test]
    fn test_engine_limit_is_total_across_pages() {
        // Like history: the newest 5, fetched 2 at a time.
        let key = key();
        let d = download(
            &key,
            request().limit(5, RequestOrder::Newest),
            2,
            vec![
                page(&key, &["a", "b"], TS, Some("off-1")),
                page(&key, &["c", "d"], TS, Some("off-2")),
                // The server says there's more, but we've hit the total.
                page(&key, &["e"], TS, Some("off-3")),
            ],
        );

        assert_eq!(d.result.unwrap(), 5);
        let limits: Vec<_> = d.requests.iter().map(|(r, _)| limit_of(r)).collect();
        assert_eq!(
            limits,
            vec![
                limit(2, RequestOrder::Newest),
                limit(2, RequestOrder::Newest),
                limit(1, RequestOrder::Newest),
            ],
            "keeps the engine's order, and the last page asks only for what's left"
        );
    }

    #[test]
    fn test_empty_collection() {
        let key = key();
        let d = download(&key, request(), 1000, vec![page(&key, &[], TS, None)]);

        assert_eq!(d.result.unwrap(), 0);
        // `on_batch` is still called once, with nothing in it.
        assert_eq!(d.batches, vec![Vec::<String>::new()]);
    }

    #[test]
    fn test_empty_page_with_offset_stops() {
        // A server bug: an offset that leads to an empty page. We must not
        // keep requesting forever.
        let key = key();
        let d = download(
            &key,
            request(),
            2,
            vec![
                page(&key, &["a", "b"], TS, Some("off-1")),
                page(&key, &[], TS, Some("off-2")),
            ],
        );

        assert_eq!(d.result.unwrap(), 2);
        assert_eq!(d.requests.len(), 2);
        assert_eq!(d.batches, vec![vec!["a", "b"], vec![]]);
    }

    #[test]
    fn test_412_on_later_page_is_collection_changed() {
        let key = key();
        let d = download(
            &key,
            request(),
            2,
            vec![
                page(&key, &["a", "b"], TS, Some("off-1")),
                error_page(precondition_failed()),
            ],
        );

        assert!(matches!(
            d.result,
            Err(Error::CollectionChangedDuringDownload)
        ));
        // The first page was already handed over; that's expected, and why
        // callers mustn't advance timestamps on error.
        assert_eq!(d.batches, vec![vec!["a", "b"]]);
    }

    #[test]
    fn test_412_on_first_page_is_a_storage_error() {
        // We don't send X-If-Unmodified-Since on the first page, so a 412 there
        // means something else is wrong.
        let key = key();
        let d = download(&key, request(), 2, vec![error_page(precondition_failed())]);

        assert!(matches!(
            d.result,
            Err(Error::StorageHttpError(
                ErrorResponse::PreconditionFailed { .. }
            ))
        ));
        assert!(d.batches.is_empty());
    }

    #[test]
    fn test_timestamp_change_is_collection_changed() {
        // The server should have sent a 412, but didn't.
        let key = key();
        let d = download(
            &key,
            request(),
            2,
            vec![
                page(&key, &["a", "b"], TS, Some("off-1")),
                page(&key, &["c"], TS + 1, None),
            ],
        );

        assert!(matches!(
            d.result,
            Err(Error::CollectionChangedDuringDownload)
        ));
        assert_eq!(
            d.batches,
            vec![vec!["a", "b"]],
            "the changed page isn't handed over"
        );
    }

    #[test]
    fn test_other_errors_are_passed_through() {
        let key = key();
        let d = download(
            &key,
            request(),
            2,
            vec![
                page(&key, &["a", "b"], TS, Some("off-1")),
                error_page(ErrorResponse::ServerError {
                    route: "storage/test".into(),
                    status: 503,
                }),
            ],
        );

        assert!(matches!(
            d.result,
            Err(Error::StorageHttpError(ErrorResponse::ServerError {
                status: 503,
                ..
            }))
        ));
    }

    #[test]
    fn test_on_batch_error_stops_the_download() {
        let key = key();
        let mut pages = vec![
            page(&key, &["a", "b"], TS, Some("off-1")),
            page(&key, &["c"], TS, None),
        ]
        .into_iter();
        let mut requests = 0;
        let result = fetch_incoming_batched(
            &key,
            request(),
            2,
            |_, _| {
                requests += 1;
                pages.next().unwrap()
            },
            |_| Err(Error::RecordTooLargeError),
        );

        assert!(matches!(result, Err(Error::RecordTooLargeError)));
        assert_eq!(
            requests, 1,
            "no more pages are fetched after on_batch fails"
        );
    }

    #[test]
    #[should_panic(expected = "batch_size must be greater than zero")]
    fn test_zero_batch_size_panics() {
        let key = key();
        download(&key, request(), 0, vec![]);
    }
}

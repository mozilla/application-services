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

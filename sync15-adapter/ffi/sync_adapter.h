#ifndef SYNC15_ADAPTER_H
#define SYNC15_ADAPTER_H

/* Generated with cbindgen:0.6.0 */

#include <stdint.h>
#include <stdlib.h>
#include <stdbool.h>

typedef struct CleartextBsoC {
  double server_modified;
  char *payload_str;
} CleartextBsoC;

typedef OutgoingChangeset *(*StoreGetUnsyncedChanges)(void*);

typedef bool (*StoreApplyReconciledChange)(void*, const char*);

typedef bool (*StoreSetLastSync)(void*, double);

typedef bool (*StoreNoteSyncFinished)(void*, double, const char*const *, size_t);

typedef struct FFIStore {
  void *user_data;
  StoreGetUnsyncedChanges get_unsynced_changes_cb;
  StoreApplyReconciledChange apply_reconciled_change_cb;
  StoreSetLastSync set_last_sync_cb;
  StoreNoteSyncFinished note_sync_finished_cb;
} FFIStore;

/*
 * Free an inbound changeset previously returned by `sync15_incoming_changeset_fetch`
 */
void sync15_incoming_changeset_destroy(IncomingChangeset *changeset);

/*
 * Get all the changes for the requested collection that have occurred since last_sync.
 * Important: Caller frees!
 */
IncomingChangeset *sync15_incoming_changeset_fetch(const Sync15Service *svc,
                                                   const char *collection_c,
                                                   double last_sync);

/*
 * Get the requested record from the changeset. `index` should be less than
 * `sync15_changeset_get_record_count`, or NULL will be returned and a
 * message logged to stderr.
 *
 * Important: Caller needs to free the returned value using `sync15_record_destroy`
 */
CleartextBsoC *sync15_incoming_changeset_get_at(const IncomingChangeset *changeset, size_t index);

/*
 * Get the number of records from an inbound changeset.
 */
size_t sync15_incoming_changeset_get_len(const IncomingChangeset *changeset);

/*
 * Get the last_sync timestamp for an inbound changeset.
 */
double sync15_incoming_changeset_get_timestamp(const IncomingChangeset *changeset);

/*
 * Create a new outgoing changeset, which requires that the server have not been
 * modified since it returned the provided `timestamp`.
 */
OutgoingChangeset *sync15_outbound_changeset_create(const char *collection, double timestamp);

/*
 * Add a record to an outgoing changeset. Returns false in the case that
 * we were unable to add the record for some reason (typically the json
 * string provided was not well-formed json).
 *
 * Note that The `record_json` should only be the record payload, and
 * should not include the BSO envelope.
 */
bool sync15_outgoing_changeset_add_record(OutgoingChangeset *changeset,
                                          const char *record_json,
                                          uint64_t modification_timestamp_ms);

/*
 * Add a tombstone to an outgoing changeset. This is equivalent to using
 * `sync15_outgoing_changeset_add_record` with a record that represents a tombstone.
 */
void sync15_outgoing_changeset_add_tombstone(OutgoingChangeset *changeset,
                                             const char *record_id,
                                             uint64_t deletion_timestamp_ms);

void sync15_outgoing_changeset_destroy(OutgoingChangeset *changeset);

/*
 * Free a record previously returned by `sync15_changeset_get_record_at`.
 */
void sync15_record_destroy(CleartextBsoC *bso);

/*
 * Create a new Sync15Service instance.
 */
Sync15Service *sync15_service_create(const char *key_id,
                                     const char *access_token,
                                     const char *sync_key,
                                     const char *tokenserver_base_url);

/*
 * Free a `Sync15Service` returned by `sync15_service_create`
 */
void sync15_service_destroy(Sync15Service *svc);

FFIStore *sync15_store_create(void *user_data,
                              StoreGetUnsyncedChanges get_unsynced_changes_cb,
                              StoreApplyReconciledChange apply_reconciled_change_cb,
                              StoreSetLastSync set_last_sync_cb,
                              StoreNoteSyncFinished note_sync_finished_cb);

void sync15_store_destroy(FFIStore *store);

bool sync15_synchronize(const Sync15Service *svc,
                        FFIStore *store,
                        const char *collection,
                        double timestamp,
                        bool fully_atomic);

#endif /* SYNC15_ADAPTER_H */

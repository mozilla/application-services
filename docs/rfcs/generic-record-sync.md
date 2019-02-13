# Generic Data Sync

Unfortunately, with the (indefinite) "pause" of Mentat, there's no obvious path
forward for new synced data types beyond 'the Sync team implements a new
component'. Presumably, at some point we decided this was both desirable, but
unworkable, hence designing Mentat. After some thought, I've come up with a plan
that gets us some of the benefits of Mentat with the following major benefits
(compared to Mentat)

- Works on top of Sync 1.5
- A couple of extensions to the Sync 1.5 server protocol would help, but are not
  necessary.
- Doesn't change the sync data model substantially.
- Doesn't require us to implement a complex database system.

## Background/Goals/Etc

In one of the AllHands, Lina had a presentation which defined three different
types of sync data stores.

1. Tree stores (bookmarks). The defining features of these stores are that:
    - They represent a tree.
    - They are considered corrupt if tree constraints are invalidated.
2. Log stores (history). The defining features of these stores are that:
    - Typically too large to fit in memory.
    - We expect to only sync a subset of the records in them.
3. Record stores (logins, addresses, credit cards, addons, etc)

This document describes a plan for syncing "Type 3" data stores in a generic
way, however extended to allow the following additional features not present in
the current system:

- Schema evolution.
- "best-effort" Inter-record references (even across collections).

### High level overview

At a high level, we will store a versioned schema somewhere on the server. This
describes both the record format, and how to perform merges. The goal is to
allow clients who have not fully been updated to perform merges without data
loss.

Additionally, the schema contains three version numbers. The first two are it's
actual version, and the minimum version a client must have to sync. These two
allow us to migrate the schema progressively, only locking out clients that are
past a certain age, while letting users with devices which are only a single
version behind sync.

The third version number is similar to the required version number, it's the
version of *the schema definition format* that clients must understand in order
to read the schema. This allows us to add support for new types in the schema,
without causing problems for schemas that do not use these types. This, unlike
the other version numbers, would be determined automatically by the library.

The schema is typed, which is required both for it to be very helpful for
merging, and for us to implement support for foreign references. Foreign
references may be to other records in the same collection, or to records in
other collections. Critically, they are a best-effort mapping, and not a hard
guarantee. In practice, it should work, however there will never be any
guarantee the record in question is even present on the machine in question.

The plan to support these is to store (somewhere, I think the lowest effort way
to do this is a new server API, but it is not the only option) information about
which guids have been renamed to which other guids. We would then use this to
fix up the set of stale guids in any collection that contains `ForeignGuid`'s
after syncing.

Locally, multiple collections are stored in a single database, and every
collection has both the local copy and mirror available, so that a 3-way-merge
may be performed.

## The Schema

Note: This has a bunch of things in it we don't need to support in the initial
version. In particular, the initial version should absolutely drop
`ForeignGuid`, `RecordSet`, and `UntypedMap`. All of these, IMO, are important
to the overall usability story here, however, our plan allows for adding new
field types/merge strategies here without locking out clients that don't use
them.

Additionally, it's worth noting that the decision not to add rich types here is
intentional. This is a representation of how the data is stored and merged, and
that's more or less all. (Note: now that we have a plan for adding new types to
this set I feel a lot less strongly here, but none-the-less)

I've written up rust code to describe the schema, which is at the bottom of the
document because it's fairly long due to many comments.

In practice, I belive schemas would look something like:

```json
{
    // sync 1.5 collection name
    "name": "passwords",
    "version": "0.1.0",

    // Optional, and defaults to the minimum version
    // still semver-compatible with 'version'
    "required_version": "0.1.0",
    "legacy": true,
    "dedupe_on": ["hostname", "username", "formSubmitURL", "httpRealm"],
    "fields": [
        { "name": "id", "type": "own_guid" },
        { "name": "hostname", "type": "text" },

        { "name": "formSubmitURL", "type": "text" },
        { "name": "httpRealm", "type": "text" },

        { "name": "username", "type": "text" },
        { "name": "password", "type": "text" },

        // In practice I don't know if these are actually deprecated.
        { "name": "usernameField", "type": "text", "deprecated": true },
        { "name": "passwordField", "type": "text", "deprecated": true },

        { "name": "timeCreated", "type": "number", "merge": "take_min", "default": 0 },
        { "name": "timePasswordChanged", "type": "number", "merge": "take_max", "default": 0 },
        { "name": "timeLastUsed", "type": "number", "merge": "take_max", "default": 0 },
        { "name": "timesUsed",  "type": "number", "merge": "take_sum", "default": 0 }
    ]
}
```

See the comments in the rust code rust code for a description of what this would
mean, what restrictions we'd impose, etc.

## Guid Renames

In order to support references to other IDs, we primarially need to handle
deduping. E.g, the guid on the server may be what a record used to be called,
however now it has a differnt name. My recommendation is a couple small changes
to the server protocol to support it, however we could also implement this
ourselves with more difficulty (and lack of transactional semantics).

### Server support

This is two parts:

1. Add a single optional field to BSOs: `prev_id`.
    - This is an optional field indicates that `prev_id` has been renamed to
      `id`, and that (once the batch is committed).
    - `prev_id` does not need to reference an ID that is known to the server,
      however it makes sense to ensure it's a valid BSO ID string.
    - This would not change the semantics of any existing APIs. Critically,
      requests to the old ID should still return whatever is (or is not) present
      for that id.

2. New endpoint: `GET https://<endpoint-url>/rename?ids=<ids>`
    - `ids` is a comma separated list of up to 100 (as with
      `{DELETE,GET} /storage/collection?ids=`)
    - Returns a JSON array of the renamed version of the ids, in the same order
      they were sent.
    - For IDs that were never renamed, or that are not known, should return them
      as-is, and for IDs that have been renamed, it should return the new ID.
      This is transitive.
    - Notes:
        - Renames are not collection specific. Any collection is allowed to
          store an ID from any other collection!
        - The length of the array should always be identical to the number of
          ids requested.
        - Renames should be resolved transitively, e.g. if `"a"` is renamed to
          `"b"`, and `"b"` is renamed to `"c"`, then `/rename?ids=a,b` should
          return `["c", "c"]`.
        - Presumably the table this is stored in on the server would have both
          `(<user_id>, a, c)` and `(<user_id>, b, c)` in it, or something along
          those lines.
    - Some thought is needed about how to expire this data. Initially (I have
      not thought for very long about this):
        - It seems reasonable to me to delete the data about the renames when
          the *transitive* destination ID is deleted (e.g. the case above should
          still work even if "b" has been deleted, but once "c" is deleted it
          seems like it can be deleted).

### Legacy client support

We would need to extend legacy clients to provide `prev_id` in the case of
renames. This seems reasonably easy to do on a best-effort basis (store the data
in memory, or even back with JSONFile).

## Clocks and Timestamps

In this system, we use vector clocks for conflict detection (where possible --
we support syncing with legacy collections that may be written to by naive
clients, so for collections explicitly marked as legacy, we fall back to using
modification timestamps in the case that vector clocks are not present).

Additionally, there are some cases where we use modification timestamps for
conflict *resolution*, but only after the vector clock has detected that a
conflict truly has occurred (or is unavailable, as may be the case when
interacting with legacy collections). This is done much in the same way as
the current sync system, and is discussed in more detail in the section on
merging records.

Anyway, vector clocks may be unfamiliar, so I'll give a brief overview of how
these work, with a focus on how we plan on using them.

### Vector clock overview

Feel free to skip this if you know how these work.

The goal of a vector clock is basically to let us detect the difference between
stale and conflicting data, which is something we have no ability to detect
currently.

Despite it's name, it is not a vector, and does not measure time. It is
basically a `HashMap<ClientId, u64>`, where ClientId is something like the local
client id generated by each client upon database creation, and the u64 is a
per-client change counter.

Wherever a client makes a change to some item (where item may be a record, a
field, etc), it increments it's current change counter, and sets the value
stored in the clock for it's ID to the current value of the change counter.

This lets you easily tell if one version of a record is an ancestor of another:
if record A's clock has an entry for every client in record B's clock, and they
all have the same value or higher, then record B is just a previous version of
record A, and vice versa. If neither is strictly greater than the other, then
a conflict has occurred.

See the following resources for more background, if desired:

- http://basho.com/posts/technical/why-vector-clocks-are-easy/
- http://basho.com/posts/technical/why-vector-clocks-are-hard/
- https://www.datastax.com/dev/blog/why-cassandra-doesnt-need-vector-clocks
- https://en.wikipedia.org/wiki/Vector_clock

### Our use of Vector clocks

One of the noted downsides of vector clocks is that they tend to grow without
bound. That is, there are cases when an item is added to a vector clock (a
client that has never modified a given record does so for the fist time), but
no cases where an item is removed from one.

- This seems likely to be a non-problem for us in practice. A record will need to
  be changed on a given client in order for that client to add it's ID to the
  clock, and in practice the number of devices remains small for most users. We
  can also implement pruning strategies if in practice this assumption turns out
  to be wrong.

#### Client IDs

As mentioned elsewhere, we'll generate a local client ID when the database is
created. This is used for the client ID for the vector clock, and in the
`__metadata__:client_info` record (See below). There's a problem here in that
users may copy ther profile around, or have it copied by their OS (on e.g. iOS).

Ideally we'd get some notification when that happens. If we do, we generate a new
client ID and be fine. In practice, this seems tricky to ensure. It seems likely
that we will want to add logic to handle the case that a client notices that
some other client is using it's client ID.

If this happens, it needs to take the following steps:

1. Generate a new client id.
2. Find all records that it has changed since the last sync:
    - Set their vector clock to be value of the vector clock stored in the
      mirror + `{new_client_id: old_vector_clock[old_client_id]}`. That is, it
      makes it appear that it had made the changes under the new client ID all
      along.

      JS pseudocode since rereading that explanation is confusing:

        ```js
        newVectorClock = Object.assign({}, mirrorVectorClock, {
            [newClientId]: oldVectorClock[oldClientId]
        });
        ```
This should be rare, but seems worth handling since it would be bad if the
system were fundamentally unable to handle it

Vector clocks do not help us perform merges. They only detect the cases where
merges are required. In practice, if CRDTs are implemented, this is not a
problem, although each CRDT will require a large number of vector clocks, and so
it's unclear if this is a good thing for our uses. At a minimum, it seems
clearly out of scope initially (eventually it's possible we will use CRDTs
to model some field types).

Instead, we store the most recent known server value in what's known as the
"mirror" database, and is discussed further in the section on merging.

It's worth noting that a newer variant of vector clocks, known as interval
tree clocks exists, which attempts to handle the finnicky nature of client
IDs. However, these are substantially more complex and harder to understand
than vector clocks, so I don't think they make sense for our use case.

#### Legacy collections

Collections that still must interact with legacy sync clients will neither
update nor round trip vector clocks. To handle this, in the case that these are
missing, we fall back to the current conflict resolution algorithm, based on
modification timestamps. This is unfortunate, but it will allow us to behave
progressively better as more clients are changed to use this system.

#### Conflict resolution

Vector clocks alone do not solve conflict resolution unless you design your data
types very specifically to take advantage of them. We do not, nor do we plan on
starting to.

Instead, most conflict resolution is schema driven, based on comparison with a
mirror record, and uses record modification timestamps where an age-based
comparison is required.

## Sync and Merging

Sync works as follows.

### Sync algorithm

This assumes there are no migrations, or only compatible migrations. The case of
incompatible migrations is described in the section on migrations.

Note that the following steps must be done transactionally (that is, if any step
fails completely, no changes should be recorded).

1. We download the schema and client_info from the server.

    - If we're locked out of the schema due to it being incompatible with our
      native schema version, then we stop here and return an error to the
      caller.

2. If we need to upgrade the local version of the schema based on the version
   from the server, we do that.

    - This may change local versions of records, but we assume identical changes
      have been made other clients.

        - This is following the principal described before that the remote data
          may not be on the latest version of the schema, but the remote data
          combined with the schema is enough to bring things up to date.

3. All changed records are downloaded from the server.

4. All tombstones in the set of changed records are applied. For each tombstone:

    1. If a local modification of the record exists:

        - If deletions are preferred to local changes, then we continue as if a
          local modification does not exist, deleting the record. (see step 4.2)

        - If updates are preferred to deletions, then we will resolve in favor
          of undeleting the tombstone.

    2. If a local record exists, but without any modifications, then we forward
       it and the mirror to the incoming tombstone.

    3. Incoming tombstones that we have never seen are persisted (in order to
       prevent them from being undeleted)

        - It's possible we will want to consider expiring these eventually,
          however such functionality is easy to add in a future update.

5. For each non-tombstone record:

    1. If we have no local record with the same guid, then we search for
       duplicates using the schema's dedupe_on.

        - If there is a duplicate, then we mark that we need to change the
          duplicate record's id to the new ID, and proceed as if we had a
          local record with this ID (and a mirror with it, if the duplicate
          had a mirror record).

    2. If the incoming record is not in conflict with our local record (see
       the section on vector clocks for how we determine conflict), then we
       forward the mirror and local records to the incoming record.

    3. If the incoming record *is* in conflict with our local record, then we
       take one of the following steps:

        1. If the local record is a tombstone and the collection is set to
           prefer deletions, we resolve in favor of the local tombstone.

            - Note that this still involves the conflict check -- tombstones
              still have clocks.

        2. If the local record is a tombstone and the collection is set to
           prefer updates, we forward the local and mirror to the incoming
           remote record.

        3. If we don't have a mirror record, we need to perform a two way merge.

        4. If we have a mirror record, and the incoming record is a descendent
           of the mirror, then we can perform a three way merge correctly. This
           is likely to be the most common case.

        5. If we have a mirror record, and the incoming record is in conflict
           with the mirror record, then we should either

            1. For non-legacy collections, discard the record.

                - This should never happen unless the client that wrote the record
                  has a bug. Otherwise, it would perform a merge with server before
                  uploading the record.

                - Note that we wipe the mirror when node reassignment/password reset
                  occur, so that case doesn't apply here.

            2. For legacy collections, which could have had the vector clock wiped
               by a legacy client, assume this is what has happened, and do a three
               way merge.

    4. If we performed a two or three way merge, and the outcome of the merge
       indicates that we must duplicate the record, then

        1. We create a new local record with identical contents (but a new ID, and
           fresh vector clock) to the current local version.

        2. Then replacing the current local and mirror with the incoming record.

    5. All new and locally changed records (including records that were not
       resolved by forwarding the local and mirror versions to the incoming
       record) must be uploaded as part of this sync.

        - Before uploading the records, we validate them against the schema. If
          validation fails, we record telemetry, and only sync the records which
          pass validation.
            - Records which aren't synced should be flagged as such (XXX: flesh this out)
            - This telemetry should be monitored, as it indicates a bug.

    6. Upon completing the upload, record the last sync time for this collection,
       and commit any changes.

        - This, unfortunately, ignores the case where the upload was split over
          multiple batches, and the first batch succeeded, but the subsequent
          batch failed. I'm not sure this is possible to handle sanely... The
          fact that this system is not intended for collections which have so
          many records that this is an issue helps, although in practice there
          will be edge-case users who do have this many records.

        - More worrying, this ignores the case where we succesfully commit
          batches, but fail to commit the database transaction locally.

        - In both cases, the records we uploaded will be strict descendents of
          our local changes, however the strategy of detecting duplicate client
          ids above assumes that if a server write occurs with our client id,
          then that means our client ID needs to change. This is not ideal.

### Merging a record

This depends on the data type and merge strategy selected.

Before performing the two or three-way merges, we perform compatible schema
migrations (filling in missing default values, etc) on all input records. This
is not treated as changing the record in a material way (it does not effect its
value in it's vector clock, or it's sync_status).

#### Three way merge algorithm

The input to this is the schema, the local, (incoming) remote, and mirror records.

All records also have their modification timestamps.

1. The local record is compared to the mirror record to produce the "local delta".
    - For each field in the record, we either have `None` (no change), or
      `Some<new value>`.

2. The remote record is compared to the mirror record to produce the "incoming delta".

3. A "merged delta" is produced as followed:

    1. All changes which *are not* part of composite fields which are only
       modified in one of the two deltas are copied into the merged delta.

    2. For each composite field containing one or more changed non-deprecated
       sub-fields:

        - If the composite root has `UntypedMerge::PreferRemote`, then prefer
          remote for all members of the composite.

        - If the composite root has `UntypedMerge::TakeNewest`, then copy the
          fields in the composite from whichever has been modified more recently
          (as determined by modification timestamps) between the local and
          incoming remote records.
            - Note that we're copying from the records, not from the deltas.

        - If the composite root has `NumberMerge::TakeMin` or
          `NumberMerge::TakeMax` as it's merge strategy, then compare the values
          of the composite root in the local and remote records numerically, and
          copy all fields in the composite from the winner to the merged delta.

    3. For remaining (non-composite) fields in both the "local delta" and
       "incoming delta", Store the result of the following in the merged delta:
        - `UntypedMerge::PreferRemote`: The remote (e.g. incoming) value is taken.
        - `UntypedMerge::TakeNewest`: The value from the record that has been
          modified more recently (based on modification timestamps) is used.
        - `UntypedMerge::Duplicate`: The three-way-merge fails indicating that
           the record must be duplicated.
        - `UntypedMerge::Composite(_)`: Already handled.
        - `NumberMerge::TakeMax`: The larger value is taken
        - `NumberMerge::TakeMin`: The smaller value is taken
        - `NumberMerge::TakeSum`: The following pseudocode is performed:
            ```
            local_field_delta = max(local_field_value - mirror_field_value, 0)
            incoming_field_delta = max(incoming_field_value - mirror_field_value, 0)
            mirror_field_value += incoming_field_delta + local_field_delta
            ```
        - `BooleanMerge::PreferTrue`: If either the local delta or remote
          delta would set the result to `true`, then the merged delta should get `true`
        - `BooleanMerge::PreferFalse`: If either the local delta or remote
          delta would set the result to `false`, then the merged delta should get `false`
        - `OwnGuid` fields are ignored.
        - `ForeignGuid` fields are merged as if by `TakeNewest`
        - `UntypedMap { prefer_deletions }`: performs a similar operation to the 3WM
          where deltas are created, and each field is merged by `TakeNewest`.
            - `prefer_deletions` is true, then if one field is set to the tombstone value,
              and the otehr
            - This is a little hand-wavey, but seems sufficiently specified,
              esp. given that we aren't planning on implementing it immediately.
        - `RecordSet { dedupe_key, prefer_deletions }`:
            - The set difference is computed between the local and mirror
            - The set difference is computed between the incoming and mirror
            - The new mirror is the `old_mirror UNION (local - mirror) UNION (remote - mirror)`
            - In the case of conflict (a new or changed record present in both local or mirror),
              the newer value is taken.
            - Note: Deletions are stored as explicit tombstones, and preferred
              over modificatons iff. prefer_deletions is true.
            - This is a little hand-wavey, but seems sufficiently specified,
              esp. given that we aren't planning on implementing it immediately.

4. The "merged delta" is applied to the mirror record to produce the new mirror
   record which will be uploaded to the server.

    - This record will have a vector clock that is a descendent of the local,
      mirror, and incoming remote clocks.

#### Two way merge algorithm

Two-way merges are not ideal. They are performed only if we don't have a mirror.
They're intended to do as little damage as possible

The input to this is the schema, the local, and incoming remote records.

1. A delta is computed between the local and incoming remote records, in
   a similar manner to the three-way-merge case. This is known as "the delta".

2. A merged record ("the merged record") is created which starts with all values
   from the local record not present in "the delta"

    - It doesn't matter if you take from local or remote here, since
      these are the fields that we just determined were equal.

2. For each composite field containing one or more non-deprecated subfields
   present in "the delta": the merge is performed roughly the same as the 3WM
   case.

    - If the composite root has `UntypedMerge::PreferRemote`, then prefer
      remote for all members of the composite.

    - If the composite root has `UntypedMerge::TakeNewest`, then copy the fields
      in the composite from whichever has been modified more recently (as
      determined by modification timestamps) between the local and incoming
      remote records into "the merged record".

        - Note that we're copying from the records, not from the deltas.

    - If the composite root has `NumberMerge::TakeMin` or `NumberMerge::TakeMax`
      as it's merge strategy, then compare the values of the composite root in
      the local and remote records numerically, and copy all fields in the
      composite from the winner to "the merged record".

3. For remaining (non-composite) fields present in "the delta",
   "incoming delta", Store the result of the following in "the merged record":
    - `UntypedMerge::PreferRemote`, `UntypedMerge::TakeNewest`, `UntypedMerge::Duplicate`, `NumberMerge::TakeMax`, `NumberMerge::TakeMin`, `BooleanMerge::PreferTrue`, `BooleanMerge::PreferFalse`, `OwnGuid`, `ForeignGuid`:
        - All handled as in 3WM.
    - `NumberMerge::TakeSum`: The larger value between local and remote is taken
        - This preserves the fact that nobody ever sees the value decrease.
    - `UntypedMap { prefer_deletions }`: The maps are merged, breaking ties in
      favor of the more recently modified.
        - if `prefer_deletions` is true, any field represented by a tombstone in
          either side is a tombstone in the output
    - `RecordSet { dedupe_key, prefer_deletions }`:
        - The result is the set union of the two, with deletions preferred if
          `prefer_deletion` is true

4. The "merged delta" is applied to the mirror record to produce the new mirror
   record which will be uploaded to the server.

    - This record will have a vector clock that is a descendent of the local,
      mirror, and incoming remote clocks.

## New metadata records:

These are not stored in meta/blah, they're stored in
`$collection/__metadata__:blah` (or, `$collection/__metadata__%3Ablah`, really).
This is to allow transactional behavior during updates, which is important for
the case of schema changes. Existing clients will need to change to ignore these
records.

(I'm willing to bikeshed over the name so long as it doesnt fit any pattern any
current client generates)

**Important**: This requires a change to current clients so that they know to ignore all items
in a collection whose ID starts with `__metadata__:`!

### `$collection/__metadata__:client_info`

Information about clients. An object with a single field currently, but possibly
more in the future (the library must make an effort to not drop fields it does not
understand when updating this record). The only field is `"clients"`, which is
an array of records, each with the following properties:

- `"id"`: A unique ID generated on DB creation. Unrelated to any sort of current client ID. Discussed in the section on counters/consistency. This is a string.

    - It's illegal for this to be duplicated. (If that happens, the `__metadata__:clients` record is considered corrupted and is discarded, I guess).

- `"native_schema_version"`: This clients "native" schema version for this collection.
    - This is a semver version string.
    - This is the latest version it was told to use locally, even if in practice it uses a more up to date schema it fetched. This is effectively the version that the code adding records understands

- `"local_schema_version"`: The latest version of the schema that this client understands.
    - This is also a semver version string.
    - This is always semver-compatible with `"native_schema_version"`.

- `"remote_schema_version"`: The last remote schema version it saw.
    - This is a semver version string.
    - If this is not semver compatible with `"native_schema_version"`, then the client is locked out and will not sync records.
        - Note: There are some exceptions, see the section on prerelease version migrations.

- `"metaschema_version"`: The version of the "metaschema", e.g. the schema description format we support. This changes when we add new features to the rust code implementing the schema format.
    - This is a semver version string, but is unrelated to `{native,local,remote}_schema_version`.

- `"last_sync"`: The most recent X-Weave-Timestamp (as returned by e.g. the fetch to `info/collections` we do before the sync or something). This is for expiring records from this list.

### `$collection/__metadata__:schema`

The most recent schema record. Has the following fields.

- `"current_version"`: A semver version string for this schema. This is provided by the
  consumers of the generic sync library.

- `"required_version"`: See the section on version numbers for the relationship
  between this and `current_version`.

- `"required_metaschema_version"`: The version of the schema description format
  that is required to understand this schema. Semver string.

    - This is not necessarially the same as the latest metaschema_version that
      rust library that wrote the schema understands, since if we add a new data
      type, it breaks compatibility with old clients *if and only if* the schema
      uses this type.

- `"schema"`: The schema payload.

## Migrations

This is a very important topic the first version of the document failed to cover.

There are several cases, all of which have different (although frequently
overlapping) concerns.

- Semver-compatible migrations for shipping code.
- Semver-incompatible migrations for shipping code.
- Migrations for pre-release code (which itself has several sub-categories of
  concerns).
- Schema format migrations

### A note on version strings/numbers

I've opted to go for semver strings in basically all cases where it's a string
that a developer would write. This is nice and familiar, and helps us out a lot
in the case of 'prerelease' versions, but there are several cases where it
doesn't make sense, or isn't enough:

- Metaschema version, where we may add features (for example, new data types)
  to the record-sync library in such a way that only schemas that use these
  features have a compatibility break.

- Locking out old clients. Ideally, you could do migrations in slowly, in
  multiple steps:

    For example, if you want to make a new mandatory field, in version X you
    start populating it, then once enough users are on X, you release a
    version Y that makes it mandatory, but locks out users who have not yet
    reached X.

    Similarly for field removal, although our design handles that more
    explicitly and generally with the `deprecated` flag on fields.

    This is more or less the reason that we never change the version number
    in meta/global. It immediately impacts every unreleased version.

For both of these, we distinguish between the `current` version, and the `required`.

This is how the two are related:

- The current version must always be greater or the same than the required version
  for the client imposing the restriction. It's nonsensical otherwise.

- The required version must be semver compatible with the "current" version, and
  by default it is the smallest version that is semver-compatible with the
  current version

This is to say, if you add a new optional "foobar" field to your record in
"0.1.2", once "0.1.2" is everywhere, you can make it mandatory in a new "0.1.3",
which is listed as requiring "0.1.2".

This has the downside of... not really being what semver means at all. So I'm
open to suggestions for alternatives.

#### Native, local, and remote versions

There's another complication here, and that's the distinction between native, local,
and remote versions.

- The "remote" schema is any schema from the server, but almost always we use it
  to mean the latest schema version.
- The "native" schema version is the version that the client would be using if it
  never synced a new schema down from the server.
- The "local" schema version is the version the client actually uses. Initially
  it's the same as the native version, and if the client syncs, and sees a
  compatible 'remote' schema, then it will use the remote schema as it's new local
  schema.

Critically, the `required` schema check (described above) is performed against the
*native* schema version, and *not* the local schema version. This is required for
the system to actually lock out older clients -- otherwise they'd just confuse
themselves (in practice they should still be locked out -- we will need to make
sure we validate all records we're about to upload against the remote schema,
but this should allow them to avoid wasting a great deal of effort and possibly
reporting error telemetry or something).

Anyway, the way this will work is that if a client's *native* (**not** local)
schema version falls behind the required version, it will stop syncing.

### Semver-compatible migrations (for shipping code)

There are two categories here: Either `dedupe_on` is unchanged/relaxed, or
additional constraints are added.

Most of the time, the server data does not need to change here. The combination
of the new schema with the data the server has (which will be semver-compatible
with the new data -- or else you need to read the next section) should be enough
when combined to give all clients (who are capable of understanding the schema)
identical results.

However, we also allow adding additional constraints to `dedupe_on`. In this case,
some records may now be duplicates of existing records. Failing to fix these may
result in different clients deciding one record or another is the canonical record,
and it's not great if they disagree, so we fix it up when uploading the schema.

#### Algorithm for increasing `dedupe_on` strictness

The client uploading the schema with the new dedupe_on restriction performs the
following steps transactionally. (That is, this all needs to be XIUS
`$when_we_fetched_the_schema` *and* should either run in memory, or in a single
database transaction, where XIUS failure is a rollback)

1. Find all combinations of records that are now considered duplicated.
    - Note that this isn't a set of pairs, it's a set of lists, e.g. changing
      `dedupe_on` could could cause any number of records to be unified.

2. For each list of records containing 2 or more records:
    1. Sort them by update_counter descending.

    2. Merge them front to back using two_way_merge until only a
      single record remains.

        - XXX: Or should we just take the one with the highest update_counter outright?

    3. The result will have the ID of the first record in the list, and will
      have a prev_id of the 2nd record.

    4. Each subsequent record will be recorded as a tombstone with a prev_id of
      the record following it (except for the last record, which will have nothing).

    For example, to merge `[a, b, c, d]`, payload of `a` will be `merge(merge(merge(a, b), c), d)`. We'd then upload (records equivalent to after adding the rest of the bso fields and encrypting)

    ```json
    [
        { "id": "a", "prev_id": "b", "payload": "see above" },
        { "id": "b", "prev_id": "c", "payload": { "deleted": true } },
        { "id": "c", "prev_id": "d", "payload": { "deleted": true } },
        { "id": "d", "payload": { "deleted": true } }
    ]
    ```

    See the proposed server extension section for information on `"prev_id"`
    (the important part is we remember the rename sequence).

3. Upload the outgoing records and (on success) commit the changes locally.

### Semver-incompatible migrations

A lot of thought has been given to allowing evolution of the schema such that
these are not frequently required. Most of the time you should be able to
either deprecate fields, or move through a compatible upgrade path and block
out the old data by using `required_version`.

However, some of the time, outright breaking schema may be unavoidable.

Fundamentally, this will probably look like our API requiring that for a
semver-major change, the code either explicitly migrating all the records (e.g.
give them a list of the old records, get the new ones back), or very explicitly
saying that the old records should be deleted.

There are a few ways to do this in the API, I won't bikeshed that here since
they aren't super important.

The big concern here is that it means that now all records on the server must go,
and be replaced. This is very unlikely to lead to happy servers, even if the
record counts are small. Instead, what I propose is as follows:

1. If the user explicitly syncs, we do the full migration right away. The danger
   here is automatic syncs, not explicit ones. We will need to adjust the API to
   allow indicating this.

2. Otherwise, use a variant of our bookmark repair throttling logic:

    - There's an N% (for N around, IDK, 10) chance every day that a given
      client does the full sync/upgrade routine.

    - If, after M days of being updated, none of the clients have done this,
      just go for it.

    - TODO: discuss this with ops for how aggressive seems sane here.

### Prerelease migrations

XXX Wrote most of this before thinking about current vs required versions,
revisit to see if it still makes sense

Evolving the schema before shipping has never been something sync has handled
well, so it's an explicit design consideration here. These features could also
be used to do A/B testing, probably.

Effectively, if you're on a prerelease version, you should have a lot more
freedom to break things. You also should have freedom to abandon that prerelease
and go back to the way things were before, without completely trashing things.

If the schema is `0.0.x`, or `x.y.z-pre.release.etc` (see
https://semver.org/#spec-item-9), then the version is considered to be a
prerelease version. Note `"0.0.x"` (and `0.0.x-foo`) have some additional
handling, under the assumption that they have never been part of a
release, and so breaking changes are normal, common, and fine.

This has a few effects:

- A client on a prerelease version that sees a remote schema with a higher
  version than it's local version will stop syncing in the following cases:

    - The version is semver-incompatible.

    - The version starts with '0.0' (e.g. `0.0.1`, `0.0.2-SNAPSHOT`, etc), even
      if it would otherwise be semver-compatible (e.g. `0.0.1` and
      `0.0.1-SNAPSHOT`).

    This is intended to allow a great deal of flexibility for iterating
    without worrying about old clients uploading stale data.

- A client on a prerelease version that sees a remote schema with a lower
  version will replace it.

- Client migration throttling logic is intentionally disabled. We assume you
  don't have enough users using prerelase versions that this is an issue. If
  you do, then they probably should not be prerelease versions.

- A client who is not on a pre-release version who sees that the server has a
  pre-release version behaves as follows:

    - If the version is semver-compatible, then it behaves as it would for any
      semver-compatible version.

        - TODO: A way to disable this might be nice. Maybe if `breaking` is in
          the version string (as in `1.2.3-breaking`)?

    - If the remote schema has not been modified in 30(?) days, then the
      prerelease version is assumed to be abandoned, and we do a full resync.

        - In the case that the version is semver-compatible, we'll merge in the
          remote changes, otherwise, we discard them.

        - Note that this is only performed in the case that the remote has a
          prerelease version, our local version is not prerelease.

        - TODO: Is there a better way to handle the TTL here?

    - It will not attempt to sync unless the prefix of the prerelease version is identical.


## Conclusion

This got pretty long. I've addressed most of the concerns from the google doc,
and moved it to github because I find it very hard to wade through many
comments in google docs, as they get pushed around.

Anyway, while it's very long, most of that is because I've been explicit about
the things we'd need to do. It's certainly a non-trivial amount of work, but a
lot of the hand-waving has been removed, hopefully.

The benefits of this is we'll be able to:

1. Implement some collections more easily.
2. Provide a path for other teams who want sync/storage functionality for new
   collections, but don't want to get a work week with the sync team, or to
   swallow the restrictions of the current sync limitations.
3. Allow current collections (e.g. logins) to have a path forward where they'd
   be evolve the server-side schema.

Most of the hard parts of this  (`ForeignGuid`, and the map and set types) can
be postponed until we need them, although I don't think they'd be that difficult
in practice.

## Appendix 1: Rough SQL schema

I've sketched out a SQL schema that would be used to store the data.

```sql
-- Table of collection info.
CREATE TABLE collections (
    id   INTEGER PRIMARY KEY,
    name      TEXT NOT NULL UNIQUE,
    -- Server last sync timestamp (1000 * sync15::ServerTimestamp),
    -- or null if we've never synced.
    last_sync INTEGER,

    local_schema_id INTEGER NOT NULL,

    remote_schema_id INTEGER,

    -- This is incremented on changes (this is actually more frequent than needed,
    -- but seems safe), and used as our clients value in the vector clock for this
    -- collection.
    update_counter INTEGER NOT NULL DEFAULT 0,

    FOREIGN KEY(coll_id) REFERENCES collections(id) ON DELETE CASCADE

    FOREIGN KEY(local_schema_id) REFERENCES schemas(id)
    FOREIGN KEY(remote_schema_id) REFERENCES schemas(id)
);

-- Table of local records
CREATE TABLE rec_local (
    id             INTEGER PRIMARY KEY,
    coll_id        INTEGER NOT NULL,
    guid           TEXT NOT NULL UNIQUE,

    record_json    TEXT NOT NULL          CHECK(json_valid(record_json)),
    -- timestamp in milliseconds since the unix epoch, or 0 if never modified locally.
    local_modified INTEGER NOT NULL DEFAULT 0 CHECK(local_modified >= 0),

    is_deleted     TINYINT NOT NULL DEFAULT 0,
    sync_status    TINYINT NOT NULL DEFAULT 0,

    vector_clock TEXT NOT NULL,

    FOREIGN KEY(coll_id) REFERENCES collections(id) ON DELETE CASCADE
);

-- Mirror table
CREATE TABLE rec_mirror (
    id             INTEGER PRIMARY KEY,
    coll_id        INTEGER NOT NULL,
    guid           TEXT NOT NULL UNIQUE,

    record_json    TEXT NOT NULL        CHECK(json_valid(record_json)),

    -- in milliseconds (a sync15::ServerTimestamp multiplied by 1000 and truncated)
    server_modified INTEGER NOT NULL CHECK(server_modified >= 0),

    -- As in logins. Whether or not there have been local changes to the record.
    is_overridden   TINYINT NOT NULL DEFAULT 0,

    vector_clock TEXT NOT NULL, -- Can be null for legacy collections...

    FOREIGN KEY(coll_id) REFERENCES collections(id) ON DELETE CASCADE
);

-- Up to two schemas are stored per collection, the local and remote one.
CREATE TABLE schema_data (
    id              INTEGER PRIMARY KEY,
    coll_id         INTEGER NOT NULL,

    is_local        TINYINT NOT NULL,
    is_legacy       TINYINT NOT NULL,

    current_version  TEXT NOT NULL,
    required_version TEXT NOT NULL,

    schema_json     TEXT NOT NULL CHECK(json_valid(schema_json)),
    FOREIGN KEY(coll_id) REFERENCES collections(id) ON DELETE CASCADE
);

-- Used to store
-- 1. our local ID
-- 2. the 'native' schema version (see notes on versions for what this means)
CREATE TABLE metadata (
    key TEXT PRIMARY KEY,
    value TEXT
) WITHOUT ROWID;
```

## Appendix 2: Rust schema code, copious comments

This explains how I think a lot of this should work, becauseI find this easier
to reason about if it's in code and not in prose. Most things are commented.

Note that I'm not sure we should use serde to derive(Deserialize) on this, since
it encodes in a really gross way (that could be fixed, but we'd express fewer
restricitons in the type system), and even if we did, errors would come through
as type errors. It seems complex enough that parsing it from json so that we can
give better error messages could very easily be worth while.

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct RecordSchema {

    /// The version of the schema
    pub version: semver::Version,

    /// The required version of the schema
    pub required_version: semver::Version,

    /// Is this a legacy collection?
    pub legacy: bool,

    /// How to merge each field.
    ///
    /// Note: Unknown fields are preserved, are merged by TakeNewest,
    /// and have no type constraints.
    pub fields: HashMap<String, FieldKind>,

    /// List of field names where if all values match, then the records should
    /// be considered 'duplicates' and deduped. Examples:
    ///
    /// - `url` for history entries
    /// - The combination of `hostname`, `username`,
    ///   `formSubmitURL`, and `httpRealm` for logins
    /// - addon id for addons.
    /// - etc.
    ///
    /// # Restricions
    /// - All fields must be present in `fields`.
    /// - Fields must not be [`FieldKind::OwnGuid`], [`FieldKind::ForeignGuid`],
    ///   [`FieldKind::UntypedMap`], or [`FieldKind::RecordSet`].
    ///     - If you think you really need OwnGuid, just leave this blank, Two
    ///       records with the same OwnGuid value don't need to be deduped,
    ///       they're already considered the same.
    /// - Hmm... more?
    pub dedupe_on: Vec<String>,
}

/// A single field in a record.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {

    /// Whether or not this field is required.
    ///
    /// Be careful about this, as removing a required field is much
    /// trickier than removing an optional one.
    pub required: bool,

    /// Whether or not this field is deprecated. Clients won't bother
    /// uploading merge resolutions that occur on deprecated fields.
    ///
    /// If a client previously considered a field required and now considers it
    /// deprecated, then we'll substitute a default value of that type (empty
    /// string, 0, etc).
    pub deprecated: bool,

    /// The kind of the field. See [`FieldKind`] for more information.
    pub kind: FieldKind,

    /// An optional default value. This must be comptaible with the type
    /// specified by FieldKind.
    pub default_value: Option<serde_json::Value>,
}

/// Represents the combination of the type of a field and how to merge it.
///
/// These sound like two different things, conceptually, however many field
/// types have restrictions on how you can merge them (for example, it is
/// nonsensical to attempt to merge two strings using numeric max).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldKind {
    /// Indicates that this field can contain any type of item representable
    /// using JSON.
    Untyped(UntypedMerge),

    /// Indicates that this field contains text or a string.
    Text(TextMerge),

    /// Indicates that this field is numeric. Numeric fields may be integers
    /// or real numbers.
    ///
    /// You will probably want to use this for timestamps and dates.
    ///
    /// TODO: Separate float / int? is 53 bits of int precision enough in
    /// practice?
    Number(NumberMerge),

    /// Indicates that this field is a boolean flag.
    Boolean(BooleanMerge),

    /// Indicates that this field is a Guid of some record other than this
    /// record. These may only be merged using age-based merge strategies.
    ///
    /// It's sometimes useful for one record be able to reference another
    /// record's guid. Unfortunately, sync will, at times, change a record's
    /// guid (for example, if the `dedupe_on` properties indicate that the
    /// record is a duplicate). Indicating that the field is a `ForeignGuid`
    /// means that sync will fix up these renames whenever it can.
    ///
    /// ForeignGuid always will behave as if it has the
    /// `UntypedMerge::TakeNewest` merge strategy
    ///
    /// # Restrictions
    ///
    /// - It's an error to use `ForeignGuid` in a schema's `dedupe_on`.
    ///
    /// # Caveats
    ///
    /// *Important*: This is done solely on a *best effort* basis. No guarantee
    /// is made that all guid renames will be detected (legacy clients may not
    /// register the renames when they occur, for example).
    ///
    /// Moreover, for foreign collections, no guarantee is possible that we will
    /// have the (for example, for referencing history entries, that record may
    /// have been expired)
    ///
    /// If possible, you are recommended instead to reference things based on
    /// content. E.g. for the example of history, store the URL instead of the
    /// place GUID.
    ForeignGuid,

    /// Indicates that this field should be used to store the record's own guid.
    ///
    /// This means the field is not stored on the server or in the database, and
    /// instead is populated before returning to the record in APIs for querying
    /// records.
    ///
    /// # Restrictions
    ///
    /// - It's an error to use `OwnGuid` in a schema's `dedupe_on`.
    OwnGuid,

    /// Indicates that this field stores a dictionary of key value pairs which
    /// should be merged individually. It's effectively for storing and merging
    /// a user defined JSON objects.
    ///
    /// This does not take a merge strategy parameter, because it implies one
    /// itself. If you would like to use a different merge strategy for
    /// json-like data, then [`UntypedMerge`] is available and appropriate.
    ///
    /// The map supports deletions. When you write to it, if your write is
    /// missing keys that are currently present in (the local version of) the
    /// map, they are assumed to be deleted.
    ///
    /// `prefer_deletions` indicates whether updates or deletions win in the
    /// case of conflict. If true, then deletions always win, even if they are
    /// older. If false, then the last write wins.
    ///
    /// # Restrictions
    ///
    /// - It's an error to use `UntypedMap` in a schema's `dedupe_on`.
    UntypedMap {
        prefer_deletions: bool,
    },

    /// A unordered set of JSON records. Records within the set will not be
    /// merged, however the set itself will be.
    ///
    /// This does not take a merge strategy parameter, because it implies one
    /// itself.
    ///
    /// The dedupe_key is the string key that is used test members of this set
    /// for uniqueness. Two members with the same value for their dedupe_key are
    /// considered identical. This is typically some UUID string you generate in
    /// your application, but could also be something like a URL or origin.
    ///
    /// The set supports deletion in so far as when you write to the set, if
    /// your write is missing items that are currently present in the (local
    /// version of the) set is assumed to be deleted.
    ///
    /// `prefer_deletions` indicates whether updates or deletions win in the
    /// case of conflict. If true, then deletions always win, even if they are
    /// older. If false, then the last write wins.
    ///
    /// # Restrictions
    ///
    /// - It's an error to use `RecordSet` in a schema's `dedupe_on`.
    RecordSet {
        dedupe_key: String,
        prefer_deletions: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UntypedMerge {
    /// Take the value for the field that was changed most recently.
    ///
    /// This is recommended for most data.
    TakeNewest,

    /// This merge strategy is used in cases where the local client does
    /// not understand the data type. It's effectively a 'pass-through'
    /// of the remote data.
    ///
    /// In most cases, this should not be specified manually.
    PreferRemote,

    /// If a conflict occurs on this field, duplicate the record.
    ///
    /// This is not recommended for most cases. Additionally, it is forbidden
    /// for records that have non-empty dedupe_on lists that do not contain this
    /// field.
    Duplicate,

    /// Use to indicate that this field is conceptually part of another field.
    ///
    /// The string parameter is used to indicate the 'root' of the composite
    /// field, which should specify the merge strategy. The following merge
    /// strategies are currently valid for composite roots:
    ///
    /// 1. [`TrivialMerge::TakeNewest`] and [`TrivialMerge::PreferRemote`]: If
    ///    the root of the composite has has one of these as it's merge
    ///    strategy, then `TakeNewest`/`PreferRemote` is performed on conflict
    ///    of *any* field of the composite.
    ///
    /// 2. [`NumberMerge::TakeMin`] and [`NumberMerge::TakeMax`]: If the root of
    ///    the composite has one of these as its merge strategy, then *only*
    ///    conflicts on the root are considered, and the way that they resolve
    ///    decides how the non-root composite members are resolved.
    ///
    /// These are subtly different (in 1, the root is not special, and in 2 it
    /// is), but tend to map to what you want.
    ///
    /// Case 1 is for compound data types where any part of them may change, but
    /// merging two records across these changes is fundamentally broken. For
    /// example, address part 1 and part 2, credit card number and expiration
    /// date, etc.
    ///
    /// Case 2 is for when one item is extra information that pretains to the
    /// first item. For example you might want to merge using
    /// [`NumberMerge::TakeMax`] for a last use timestamp, and also some
    /// information about the use -- for example, which device it occurred on.
    /// Storing the information about the device as a composite field rooted on
    /// a `TakeMax` field which stores the use timestamp will ensure that the
    /// two are changed together.
    Composite(String),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextMerge {
    /// Text may be merged using any of the [`UntypedMerge`] techniques.
    Untyped(UntypedMerge),
    // Anything else?
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NumberMerge {
    /// Numbers may be merged using any of the [`UntypedMerge`] techniques.
    Untyped(UntypedMerge),
    /// Take the minimum value between the two fields.
    ///
    /// Use this for things like creation timestamps, where a smaller number
    /// always wins.
    TakeMin,
    /// Take the maximum value between the two fields.
    ///
    /// Use this for things like last use timestamps, where a larger number
    /// always wins.
    TakeMax,

    /// Treat the value as if it's a rolling sum. This actually does something
    /// like `out.field += max(remote.field - mirror.field, 0)` (e.g. it does
    /// the right thing).
    ///
    /// Use this for things like use counters and similar.
    TakeSum,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BooleanMerge {
    /// Booleans may be merged using any of the [`UntypedMerge`] techniques.
    Untyped(UntypedMerge),

    /// On conflict, if either record is set to `false`, then the output is `false`.
    ///
    /// This is equivalent to a boolean "and" operation.
    PreferFalse,

    /// On conflict, if either record is set to `true`, then the output is `true`.
    ///
    /// This is equivalent to a boolean "or" operation.
    PreferTrue,
}
```

# Sync data model

- Introduce high-level concepts here, and elaborate on the details elsewhere?

## Data classes

All synced data types fit into one of three broad categories.

- Are there others we should think about?
- Would a table comparing the different characteristics be useful?
- These classifications aren't rigid; it'd be nice to find a way to describe that better.

### Hierarchical data

Bookmarks are an example of hierarchical data, or **trees**. Records in tree collections are interdependent: parents have pointers to their children, and children back to their parents.

Hierarchical data present a problem for syncing, because records can be downloaded in any order. Additionally, some changes require multiple records to be uploaded in lockstep. Missing or incomplete records lead to problems like missing parents and children, and parent-child disagreements, which clients must resolve to ensure the server data remain consistent.

### Append-only data

History is an example of append-only, or **log** data. Since all entries are distinct, there are no conflicts: two visits to the same page on two different devices are still two distinct visits. These records are independent, and can be synced in any order.

Log data are the easiest to sync, but the problem is _volume_. We currently limit history to the last 20 visits per page, cap initial syncs to the last 30 days or 5,000 pages, and expire records that aren't updated on the server after 60 days. These limitations are for efficiency as much as for historical reasons. Clients don't need to process thousands of entries on each sync, and the server avoids bloated indexes for large collections. Unfortunately, this is also a form of data loss, as the server never has the complete history.

### Semistructured data

Logins, addresses, and credit cards are examples of semistructured, or **document** data. Like log data, document data are independent, and can be synced in any order relative to each other. However, they _can_ conflict if two clients change the same field.

Engines that implement three-way merging support per-field conflict resolution, since they store the value that was last uploaded to the server. Engines that don't resolve conflicts at the record level, based on the timestamp.

- Explain why two-way merge can lead to data loss.

Documents _can_ refer to other records; for example, credit cards have an "address hint" field that points to a potential address record for the card. These kinds of cross-record identifiers seem similar to foreign key constraints, though they have much weaker guarantees. They aren't stable, and can't be enforced by the server or other clients. Each client must expect to handle stale, nonexistent, and inconsistent identifiers.

- Link to Thom's [generic sync proposal](https://github.com/mozilla/application-services/pull/658) with details.

## Change tracking

Each client must track changes to synced data, so that they can be uploaded during the next sync. How this is done depends on the client, the data type, and the underlying store.

## Merging

- Two-way vs. three-way merging.

## Server record format

On the server, each piece of synced data is stored as a Basic Storage Object, or BSO. A BSO is a JSON envelope that contains an encrypted blob, which is itself a JSON string when decrypted. BSOs are grouped together in buckets called collections, where each BSO belonging to the same collection has the same structure.

BSOs are typically referenced as `collection/id`: for example, `meta/global` means "the BSO `global` in the `meta` collection." Most BSOs have a random, globally unique identifier, but some, like `meta/global` and `crypto/keys`, have well-known names.

BSOs are encrypted by clients as they're uploaded, and decrypted by other clients as they're downloaded. This means the server can't see the contents of any Firefox Sync records, except for their collection names, IDs, and last modified timestamps.

- Example of record
- Encryption scheme...some overlap with life of a sync?

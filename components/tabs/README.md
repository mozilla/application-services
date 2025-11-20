# Synced Tabs Component

## Implementation Overview


This crate implements storage and a syncing engine for remote tabs, including
for "tab commands" (such as closing a remote tab).

It uses a sqlite database, but only creates the database when remote tabs need
to be stored; eg, just telling this component about local tabs will not create a database.

It is used on Desktop and mobile and uses UniFFI.

## Directory structure
The relevant directories are as follows:

- `src`: The meat of the library. This contains cross-platform rust code that
  implements the syncing of tabs.
- `android`: A kotlin wrapper we should try and remove.

## Business Logic

Each client tells the component about its local tabs, and can query for other devices and their
tabs.

This all then feeds into the single Sync payload for tabs. The core shape is very old,
predating this component. b/w compat concerns always apply, but can safely be "upgraded"
over time.

Tab commands add a layer of complexity - if we've been asked to close a remote tab, we pretend
that tab doesn't exist on the remote for some period, giving that remote device a chance to act
on the request and re-upload its tabs.

## Payload format

The sync payload has 3 distinct concepts:

* Tabs: a title, a URL history (think back button), whether it is pinned/active etc, if it is in a tab group, etc.
* Windows: So remote clients can display our tabs based on Window.
* Tab groups: So tab-groups can be recreated (or at last reflected) on remote devices.

These are distinct data-structures - eg, a tab has a "window id" and a "tab group id", and there
are separate maps for these groups and windows.

### Association with device IDs

Each remote tabs sync record is associated to a "client" using a `client_id` field, which is really a foreign-key to a `clients` collection record.
However, because we'd like to move away from the clients collection, which is why this crate associates these records with Firefox Accounts device ids.
Currently for platforms using the sync-manager provided in this repo, the `client_id` is really the Firefox Accounts device ID and all is well, however for older platforms it is a distinct ID, which is why we have to feed the `clients` collection to this Tabs Sync engine to associate the correct Firefox Account device id.

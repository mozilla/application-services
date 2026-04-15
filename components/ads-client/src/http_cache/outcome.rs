/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

#[derive(Debug)]
pub enum CacheOutcome {
    CleanupFailed(rusqlite::Error), // cleaning expired objects failed
    Hit,                            // cache hit
    LookupFailed(rusqlite::Error),  // cache miss path due to lookup error
    MissNotCacheable,               // policy says "don't store"
    MissStored,                     // stored successfully
    NoCache,                        // send policy requested a cache bypass
    StoreFailed(rusqlite::Error),   // insert/upsert failed
    TrimFailed(rusqlite::Error),    // size trim failed
}

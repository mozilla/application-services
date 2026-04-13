/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::HttpCacheError;

#[derive(Debug)]
pub enum CacheOutcome {
    CleanupFailed(HttpCacheError), // cleaning expired objects failed
    Hit,                           // cache hit
    LookupFailed(rusqlite::Error), // cache miss path due to lookup error
    MissNotCacheable,              // policy says "don't store"
    MissStored,                    // stored successfully
    NoCache,                       // send policy requested a cache bypass
    StoreFailed(HttpCacheError),   // insert/upsert failed
}

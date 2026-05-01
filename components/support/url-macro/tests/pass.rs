/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use url::Url;
use url_macro::url;

fn main() {
    let prod: Url = url!("https://ads.mozilla.org/v1/");
    assert_eq!(prod.scheme(), "https");
    assert_eq!(prod.host_str(), Some("ads.mozilla.org"));
    assert_eq!(prod.path(), "/v1/");

    let staging: Url = url!("https://ads.allizom.org/v1/");
    assert_eq!(staging.scheme(), "https");
    assert_eq!(staging.host_str(), Some("ads.allizom.org"));

    let with_query: Url = url!("https://example.com/path?key=value#frag");
    assert_eq!(with_query.query(), Some("key=value"));
    assert_eq!(with_query.fragment(), Some("frag"));
}

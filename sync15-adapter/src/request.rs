/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use util::ServerTimestamp;

use std::fmt;
use url::{Url, UrlQuery, form_urlencoded::Serializer};
use error::{self, Result};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RequestOrder { Oldest, Newest, Index }

impl fmt::Display for RequestOrder {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &RequestOrder::Oldest => f.write_str("oldest"),
            &RequestOrder::Newest => f.write_str("newest"),
            &RequestOrder::Index => f.write_str("index")
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
    pub fn new<S>(collection: S) -> CollectionRequest where S: Into<String> {
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
    pub fn ids<V>(&mut self, v: V) -> &mut CollectionRequest where V: Into<Vec<String>> {
        self.ids = Some(v.into());
        self
    }

    #[inline]
    pub fn full(&mut self) -> &mut CollectionRequest {
        self.full = true;
        self
    }

    #[inline]
    pub fn older_than(&mut self, ts: ServerTimestamp) -> &mut CollectionRequest {
        self.older = Some(ts);
        self
    }

    #[inline]
    pub fn newer_than(&mut self, ts: ServerTimestamp) -> &mut CollectionRequest {
        self.newer = Some(ts);
        self
    }

    #[inline]
    pub fn sort_by(&mut self, order: RequestOrder) -> &mut CollectionRequest {
        self.order = Some(order);
        self
    }

    #[inline]
    pub fn limit(&mut self, num: usize) -> &mut CollectionRequest {
        self.limit = num;
        self
    }

    #[inline]
    pub fn batch<S>(&mut self, batch: S) -> &mut CollectionRequest where S: Into<String> {
        self.batch = Some(batch.into());
        self
    }

    #[inline]
    pub fn batch_start(&mut self) -> &mut CollectionRequest {
        self.batch("true")
    }

    #[inline]
    pub fn commit(&mut self) -> &mut CollectionRequest {
        self.commit = true;
        self
    }

    fn build_query(&self, pairs: &mut Serializer<UrlQuery>) {
        if self.full {
            pairs.append_pair("full", "1");
        }
        if self.limit > 0 {
            pairs.append_pair("limit", &format!("{}", self.limit));
        }
        if let &Some(ref ids) = &self.ids {
            pairs.append_pair("ids", &ids.join(","));
        }
        if let &Some(ref batch) = &self.batch {
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
        base_url.path_segments_mut()
                .map_err(|_| error::unexpected("Not base URL??"))?
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

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_url_building() {
        let base = Url::parse("https://example.com/sync").unwrap();
        let empty = CollectionRequest::new("foo").build_url(base.clone()).unwrap();
        assert_eq!(empty.as_str(), "https://example.com/sync/storage/foo");
        let batch_start = CollectionRequest::new("bar").batch_start()
                                                       .build_url(base.clone()).unwrap();
        assert_eq!(batch_start.as_str(), "https://example.com/sync/storage/bar?batch=true");
        let batch_commit = CollectionRequest::new("asdf").batch("1234abcdefgh").commit()
                                                         .build_url(base.clone())
                                                         .unwrap();
        assert_eq!(batch_commit.as_str(),
            "https://example.com/sync/storage/asdf?batch=1234abcdefgh&commit=true");

        let idreq = CollectionRequest::new("wutang").full().ids(vec!["rza".into(), "gza".into()])
                                                 .build_url(base.clone()).unwrap();
        assert_eq!(idreq.as_str(), "https://example.com/sync/storage/wutang?full=1&ids=rza%2Cgza");

        let complex = CollectionRequest::new("specific").full().limit(10).sort_by(RequestOrder::Oldest)
                                                        .older_than(ServerTimestamp(9876.54))
                                                        .newer_than(ServerTimestamp(1234.56))
                                                        .build_url(base.clone()).unwrap();
        assert_eq!(complex.as_str(),
            "https://example.com/sync/storage/specific?full=1&limit=10&older=9876.54&newer=1234.56&sort=oldest");

    }
}

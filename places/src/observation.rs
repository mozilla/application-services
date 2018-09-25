/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use types::*;
use url::{Url};
use storage::{PageId}; // XXX - this should probably be in types.rs?

// An "observation" based model for updating history.
// You create a VisitObservation, call functions on it which correspond
// with what you observed. The page will then be updated using this info.
//
// It's implemented such that the making of an "observation" is itself
// significant - it records what specific changes should be made to storage.
pub struct VisitObservation {
    pub page_id: PageId,
    title: Option<String>,
    visit_type: Option<VisitTransition>,
    is_error: Option<()>,
    is_redirect_source: Option<()>,
    at: Option<Timestamp>,
    referrer: Option<Url>,
    is_remote: Option<()>,
}

impl VisitObservation {
    pub fn new(page_id: PageId) -> Self {
        VisitObservation {
            page_id,
            title: None,
            visit_type: None,
            is_error: None,
            is_redirect_source: None,
            at: None,
            referrer: None,
            is_remote: None
        }
    }

    pub fn get_url(&self) -> Option<&Url> {
        match &self.page_id {
            PageId::Url(url) => Some(url),
            _ => None
        }
    }

    pub fn title(mut self, s: String) -> Self {
        assert!(self.title.is_none(), "don't call this twice");
        self.title = Some(s);
        self
    }
    pub fn get_title(&self) -> Option<&String> {
        match self.title {
            Some(ref title) => Some(title),
            None => None,
        }
    }

    pub fn visit_type(mut self, vt: VisitTransition) -> Self {
        self.visit_type = Some(vt);
        self
    }
    pub fn get_visit_type(&self) -> Option<VisitTransition> {
        self.visit_type
    }

    pub fn at(mut self, ts: Timestamp) -> Self {
        self.at = Some(ts);
        self
    }
    pub fn get_at(&self) -> Option<Timestamp> {
        self.at
    }

    pub fn is_error(mut self) -> Self {
        assert!(self.is_error.is_none(), "don't call this twice");
        self.is_error = Some(());
        self
    }
    pub fn get_is_error(&self) -> bool {
        self.is_error.is_some()
    }

    pub fn is_remote(mut self) -> Self {
        assert!(self.is_remote.is_none(), "don't call this twice");
        self.is_remote = Some(());
        self
    }

    pub fn get_is_remote(&self) -> bool{
        self.is_remote.is_some()
    }

    // possibly used for frecency.
    pub fn is_permanent_redirect_source(mut self) -> Self {
        assert!(self.is_redirect_source.is_none(), "don't call this twice");
        self.is_redirect_source = Some(());
        self
    }

    pub fn referrer(mut self, ref_url: Url) -> Self {
        assert!(self.referrer.is_none(), "don't call this twice");
        self.referrer = Some(ref_url);
        self
    }

    pub fn get_referrer(&self) -> Option<&Url> {
        self.referrer.as_ref()
    }

    pub fn get_is_permanent_redirect_source(&self) -> bool {
        self.is_redirect_source.is_some()
    }

    // Other helpers which can be derived.
    pub fn get_was_typed(&self) -> bool {
        match self.visit_type {
            Some(VisitTransition::Typed) => true,
            _ => false,
        }
    }

    pub fn get_redirect_frecency_boost(&self) -> bool {
        self.is_redirect_source.is_some() &&
        match self.visit_type {
            Some(t) => t != VisitTransition::Typed,
            _ => true,
        }
    }

    // Other helpers which can be derived.
    // nsHistory::GetHiddenState()
    pub fn get_is_hidden(&self) -> bool {
        match self.visit_type {
            Some(visit_type) =>
                self.is_redirect_source.is_some() ||
                visit_type == VisitTransition::FramedLink ||
                visit_type == VisitTransition::Embed,
            None => false,
        }
    }
}

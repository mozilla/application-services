/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// An "observation" based model for updating history.
// You create a VisitObservation, call functions on it which correspond
// with what you observed. The page will then be updated using this info.
//
// It's implemented such that the making of an "observation" is itself
// significant. A sql storage would be expected to update one or more records,
// while a mentat-like storage would know exactly what changes to write.

use types::*;
use url::{Url};
use storage::{PageId}; // XXX - this should probably be in types.rs?

pub struct VisitObservation {
    pub page_id: PageId,
    title: Option<String>,
    visit_type: Option<VisitTransition>,
    was_typed: Option<()>,
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
            was_typed: None,
            is_error: None,
            is_redirect_source: None,
            at: None,
            referrer: None,
            is_remote: None
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

    pub fn was_typed(mut self) -> Self {
        assert!(self.was_typed.is_none(), "don't call this twice");
        self.was_typed = Some(());
        self
    }
    pub fn get_was_typed(&self) -> bool {
        match self.was_typed {
            Some(_) => true,
            None => false,
        }
    }

    pub fn is_error(mut self) -> Self {
        assert!(self.is_error.is_none(), "don't call this twice");
        self.is_error = Some(());
        self
    }
    pub fn get_is_error(&self) -> bool {
        match self.is_error {
            Some(_) => true,
            None => false,
        }
    }

    pub fn is_remote(mut self) -> Self {
        assert!(self.is_remote.is_none(), "don't call this twice");
        self.is_remote = Some(());
        self
    }

    pub fn get_is_remote(&self) -> bool{
        match self.is_remote {
            Some(_) => true,
            None => false,
        }
    }

    // XXXX - redir source and frecency needs more thought.
    pub fn is_redirect_source(mut self) -> Self {
        assert!(self.is_redirect_source.is_none(), "don't call this twice");
        self.is_redirect_source = Some(());
        self
    }

    pub fn get_is_redirect_source(& self) -> bool {
        match self.is_redirect_source {
            Some(_) => true,
            None => false,
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

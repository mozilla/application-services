/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::types::*;
use types::Timestamp;
use url::Url;

/// An "observation" based model for updating history.
/// You create a VisitObservation, call functions on it which correspond
/// with what you observed. The page will then be updated using this info.
///
/// It's implemented such that the making of an "observation" is itself
/// significant - it records what specific changes should be made to storage.
/// For example, instead of simple bools with defaults (where you can't
/// differentiate explicitly setting a value from the default value), we use
/// Option<bool>, with the idea being it's better for a store shaped like Mentat.
///
/// It exposes a "builder api", but for convenience, that API allows Options too.
/// So, eg, `.with_title(None)` or `with_is_error(None)` is allowed but records
/// no observation.
#[derive(Clone, Debug)]
pub struct VisitObservation {
    pub url: Url,
    pub title: Option<String>,
    pub visit_type: Option<VisitType>,
    pub is_error: Option<bool>,
    pub is_redirect_source: Option<bool>,
    pub is_permanent_redirect_source: Option<bool>,
    pub at: Option<Timestamp>,
    pub referrer: Option<Url>,
    pub is_remote: Option<bool>,
    pub preview_image_url: Option<Url>,
}

impl VisitObservation {
    pub fn new(url: Url) -> Self {
        VisitObservation {
            url,
            title: None,
            visit_type: None,
            is_error: None,
            is_redirect_source: None,
            is_permanent_redirect_source: None,
            at: None,
            referrer: None,
            is_remote: None,
            preview_image_url: None,
        }
    }

    // A "builder" API to sanely build an observation. Note that this can be
    // called with Option<String> (and if None will effectively be a no-op)
    // or directly with a string.
    pub fn with_title(mut self, t: impl Into<Option<String>>) -> Self {
        self.title = t.into();
        self
    }

    pub fn with_visit_type(mut self, t: impl Into<Option<VisitType>>) -> Self {
        self.visit_type = t.into();
        self
    }

    pub fn with_is_error(mut self, v: impl Into<Option<bool>>) -> Self {
        self.is_error = v.into();
        self
    }

    pub fn with_is_redirect_source(mut self, v: impl Into<Option<bool>>) -> Self {
        self.is_redirect_source = v.into();
        self
    }

    pub fn with_is_permanent_redirect_source(mut self, v: impl Into<Option<bool>>) -> Self {
        self.is_permanent_redirect_source = v.into();
        self
    }

    pub fn with_at(mut self, v: impl Into<Option<Timestamp>>) -> Self {
        self.at = v.into();
        self
    }

    pub fn with_is_remote(mut self, v: impl Into<Option<bool>>) -> Self {
        self.is_remote = v.into();
        self
    }

    pub fn with_referrer(mut self, v: impl Into<Option<Url>>) -> Self {
        self.referrer = v.into();
        self
    }

    pub fn with_preview_image_url(mut self, v: impl Into<Option<Url>>) -> Self {
        self.preview_image_url = v.into();
        self
    }

    // Other helpers which can be derived.
    pub fn get_redirect_frecency_boost(&self) -> bool {
        self.is_redirect_source.is_some()
            && match self.visit_type {
                Some(t) => t != VisitType::Typed,
                _ => true,
            }
    }

    // nsHistory::GetHiddenState()
    pub fn get_is_hidden(&self) -> bool {
        match self.visit_type {
            Some(visit_type) => {
                self.is_redirect_source.is_some()
                    || visit_type == VisitType::FramedLink
                    || visit_type == VisitType::Embed
            }
            None => false,
        }
    }
}

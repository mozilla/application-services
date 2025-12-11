/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub fn slug(text: &str) -> String {
    text.replace(|ch: char| !ch.is_alphanumeric(), "-")
        .to_ascii_lowercase()
}

pub struct UrlBuilder {
    base_url: String,
    params: Vec<String>,
}

impl UrlBuilder {
    pub fn new_dashboard(dashboard_uid: String) -> Self {
        Self {
            base_url: format!("https://yardstick.mozilla.org/d/{dashboard_uid}"),
            params: vec![],
        }
    }

    pub fn with_param(mut self, name: impl Into<String>, val: impl Into<String>) -> Self {
        self.params.push(format!("{}={}", name.into(), val.into()));
        self
    }

    pub fn with_time_range_param(mut self) -> Self {
        self.params.push("${__url_time_range}".into());
        self
    }

    pub fn build(self) -> String {
        if self.params.is_empty() {
            self.base_url.clone()
        } else {
            format!("{}?{}", self.base_url, self.params.join("&"))
        }
    }
}

/// Used to implement `join()` for an iterator
pub trait Join {
    fn join(self, sep: &str) -> String;
}

impl<T, I> Join for T
where
    T: Iterator<Item = I>,
    I: Into<String>,
{
    fn join(self, sep: &str) -> String {
        self.map(I::into).collect::<Vec<String>>().join(sep)
    }
}

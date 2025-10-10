/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Error reporting telemetry.
//!
//! Right now this uses UniFFI to forward messages to Android, which then report them to Glean.
//! In the future, once we're in the monorepo, we can hopefully get Rust to handle everything.

use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;

static RECENT_BREADCRUMBS: Mutex<BreadcrumbRingBuffer> = Mutex::new(BreadcrumbRingBuffer::new());
static LISTENER: OnceLock<Arc<dyn ErrorListener>> = OnceLock::new();

#[derive(Debug, uniffi::Error, thiserror::Error)]
pub enum ErrorListenerError {
    #[error("Unexpected: {0}")]
    Unexpected(String),
}

#[uniffi::export(with_foreign)]
pub trait ErrorListener: Send + Sync {
    fn on_error(
        &self,
        error_type: String,
        details: String,
        breadcrumbs: Vec<String>,
    ) -> Result<(), ErrorListenerError>;
}

#[uniffi::export]
pub fn register_error_listener(listener: Arc<dyn ErrorListener>) {
    if LISTENER.set(listener).is_err() {
        crate::error!("register_error_listener called multiple times");
    }
}

pub fn on_breadcrumb(breadcrumb: &str) {
    RECENT_BREADCRUMBS.lock().push(breadcrumb.to_string());
}

pub fn on_error(error_type: &str, description: &str) {
    let Some(listener) = LISTENER.get() else {
        return;
    };
    if let Err(e) = listener.on_error(
        error_type.to_owned(),
        description.to_owned(),
        RECENT_BREADCRUMBS.lock().get_breadcrumbs(),
    ) {
        crate::error!("telemetry error: {e}");
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for ErrorListenerError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Unexpected(e.reason)
    }
}

/// Ring buffer implementation that we use to store the most recent 20 breadcrumbs
#[derive(Default)]
struct BreadcrumbRingBuffer {
    breadcrumbs: Vec<String>,
    pos: usize,
}

impl BreadcrumbRingBuffer {
    const MAX_ITEMS: usize = 20;

    const fn new() -> Self {
        Self {
            breadcrumbs: Vec::new(),
            pos: 0,
        }
    }

    fn push(&mut self, breadcrumb: impl Into<String>) {
        let breadcrumb = breadcrumb.into();
        if self.breadcrumbs.len() < Self::MAX_ITEMS {
            self.breadcrumbs.push(breadcrumb);
        } else {
            self.breadcrumbs[self.pos] = breadcrumb;
            self.pos = (self.pos + 1) % Self::MAX_ITEMS;
        }
    }

    fn get_breadcrumbs(&self) -> Vec<String> {
        let mut breadcrumbs = Vec::from(&self.breadcrumbs[self.pos..]);
        breadcrumbs.extend(self.breadcrumbs[..self.pos].iter().map(|s| s.to_string()));
        breadcrumbs
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_buffer() {
        let mut buf = BreadcrumbRingBuffer::default();
        buf.push("00");
        buf.push("01");
        buf.push("02");
        buf.push("03");
        buf.push("04");
        buf.push("05");
        buf.push("06");
        buf.push("07");
        buf.push("08");
        buf.push("09");
        assert_eq!(
            buf.get_breadcrumbs(),
            vec![
                "00".to_string(),
                "01".to_string(),
                "02".to_string(),
                "03".to_string(),
                "04".to_string(),
                "05".to_string(),
                "06".to_string(),
                "07".to_string(),
                "08".to_string(),
                "09".to_string(),
            ]
        );

        buf.push("10");
        buf.push("11");
        buf.push("12");
        buf.push("13");
        buf.push("14");
        buf.push("15");
        buf.push("16");
        buf.push("17");
        buf.push("18");
        buf.push("19");
        assert_eq!(
            buf.get_breadcrumbs(),
            vec![
                "00".to_string(),
                "01".to_string(),
                "02".to_string(),
                "03".to_string(),
                "04".to_string(),
                "05".to_string(),
                "06".to_string(),
                "07".to_string(),
                "08".to_string(),
                "09".to_string(),
                "10".to_string(),
                "11".to_string(),
                "12".to_string(),
                "13".to_string(),
                "14".to_string(),
                "15".to_string(),
                "16".to_string(),
                "17".to_string(),
                "18".to_string(),
                "19".to_string(),
            ]
        );

        buf.push("20");
        assert_eq!(
            buf.get_breadcrumbs(),
            vec![
                "01".to_string(),
                "02".to_string(),
                "03".to_string(),
                "04".to_string(),
                "05".to_string(),
                "06".to_string(),
                "07".to_string(),
                "08".to_string(),
                "09".to_string(),
                "10".to_string(),
                "11".to_string(),
                "12".to_string(),
                "13".to_string(),
                "14".to_string(),
                "15".to_string(),
                "16".to_string(),
                "17".to_string(),
                "18".to_string(),
                "19".to_string(),
                "20".to_string(),
            ]
        );

        buf.push("21");
        buf.push("22");
        buf.push("23");
        buf.push("24");
        buf.push("25");
        assert_eq!(
            buf.get_breadcrumbs(),
            vec![
                "06".to_string(),
                "07".to_string(),
                "08".to_string(),
                "09".to_string(),
                "10".to_string(),
                "11".to_string(),
                "12".to_string(),
                "13".to_string(),
                "14".to_string(),
                "15".to_string(),
                "16".to_string(),
                "17".to_string(),
                "18".to_string(),
                "19".to_string(),
                "20".to_string(),
                "21".to_string(),
                "22".to_string(),
                "23".to_string(),
                "24".to_string(),
                "25".to_string(),
            ]
        );
    }
}

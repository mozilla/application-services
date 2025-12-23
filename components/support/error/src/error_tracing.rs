/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use parking_lot::Mutex;

static GLOBALS: Mutex<Globals> = Mutex::new(Globals::new());

pub fn report_error_to_app(type_name: String, message: String) {
    // First step: perform all actions that we need the lock for.
    let breadcrumbs = {
        let mut globals = GLOBALS.lock();
        if !globals
            .rate_limiter
            .should_send_report(&type_name, Instant::now())
        {
            return;
        }
        globals.breadcrumbs.get_breadcrumbs()
    };
    // Report errors by sending a tracing event to the `app-services-error-reporter::error` target.
    //
    // Applications should register for these events and send a glean error ping when they occur.
    //
    // breadcrumbs will be sent in the `breadcrumbs` field as a single string, with each individual
    // breadcrumb joined by newlines.
    let breadcrumbs = breadcrumbs.join("\n");
    tracing_support::error!(target: "app-services-error-reporter::error", message, type_name, breadcrumbs);
}

pub fn report_breadcrumb(message: String, module: String, line: u32, column: u32) {
    // When we see a breadcrumb:
    //   - Push it to the `RECENT_BREADCRUMBS` list
    //   - Send out the `app-services-error-reporter::breadcrumb`.  Applications can register for
    //     these events and log them.
    GLOBALS.lock().breadcrumbs.push(message.clone());
    tracing_support::info!(target: "app-services-error-reporter::breadcrumb", message, module, line, column);
}

// Global structs used for error reporting
struct Globals {
    breadcrumbs: BreadcrumbRingBuffer,
    rate_limiter: RateLimiter,
}

impl Globals {
    const fn new() -> Self {
        Self {
            breadcrumbs: BreadcrumbRingBuffer::new(),
            rate_limiter: RateLimiter::new(),
        }
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
        let breadcrumb = truncate_breadcrumb(breadcrumb.into());
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

fn truncate_breadcrumb(breadcrumb: String) -> String {
    // Limit breadcrumbs to 100 chars so that they fit in a Glean String list
    // (https://mozilla.github.io/glean/book/reference/metrics/string_list.html)
    if breadcrumb.len() <= 100 {
        return breadcrumb;
    }
    let split_point = (0..=100)
        .rev()
        .find(|i| breadcrumb.is_char_boundary(*i))
        .unwrap_or(0);
    breadcrumb[0..split_point].to_string()
}

/// Rate-limits error reports per component by type to a max of 20/hr
///
/// This uses the simplest algorithm possible.  We could use something like a token bucket to allow
/// for a small burst of errors, but that doesn't seem so useful.  In that scenario, the first
/// error report is the one we want to fix.
struct RateLimiter {
    // Optional so we can make `new()` const.
    last_report: Option<HashMap<String, Instant>>,
}

impl RateLimiter {
    // Rate limit reports if they're within 3 minutes of each other.
    const INTERVAL: Duration = Duration::from_secs(180);

    const fn new() -> Self {
        Self { last_report: None }
    }

    fn should_send_report(&mut self, error_type: &str, now: Instant) -> bool {
        let component = error_type.split("-").next().unwrap();
        let last_report = self.last_report.get_or_insert_with(HashMap::default);

        if let Some(last_report) = last_report.get(component) {
            match now.checked_duration_since(*last_report) {
                // Not enough time has passed, rate-limit the report
                Some(elapsed) if elapsed < Self::INTERVAL => {
                    return false;
                }
                // For all other cases, fall through and allow the report to be sent.
                //
                // Note: this also covers the `None` case which happens when the clock is
                // non-monotonic.  This shouldn't happen often, but it's possible after the OS syncs
                // with NTP, if users manually adjust their clocks, etc.  Letting an extra event
                // through seems okay in this case. We should get back into a good state soon
                // after.
                _ => (),
            }
        }
        last_report.insert(component.to_string(), now);
        true
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

    #[test]
    fn test_truncate_breadcrumb() {
        // These don't need to be truncated
        assert_eq!(truncate_breadcrumb("0".repeat(99)).len(), 99);
        assert_eq!(truncate_breadcrumb("0".repeat(100)).len(), 100);
        // This one needs truncating
        assert_eq!(truncate_breadcrumb("0".repeat(101)).len(), 100);
        // This one needs truncating and we need to make sure don't truncate in the middle of the
        // fire emoji, which is multiple bytes long.
        assert_eq!(truncate_breadcrumb("0".repeat(99) + "🔥").len(), 99);
    }

    #[test]
    fn test_rate_limiter() {
        let mut rate_limiter = RateLimiter::new();
        let start = Instant::now();
        let min = Duration::from_secs(60);
        // The first error report is okay
        assert!(rate_limiter.should_send_report("test-type", start));
        // The report should be rate limited until 3 minutes pass, then we can send another one.
        // Add time from the instant to simulate time going forward.
        assert!(!rate_limiter.should_send_report("test-type", start));
        assert!(!rate_limiter.should_send_report("test-type", start + min * 1));
        assert!(!rate_limiter.should_send_report("test-type", start + min * 2));
        assert!(rate_limiter.should_send_report("test-type", start + min * 3));
        assert!(!rate_limiter.should_send_report("test-type", start + min * 4));
        assert!(!rate_limiter.should_send_report("test-type", start + min * 5));
        assert!(rate_limiter.should_send_report("test-type", start + min * 6));

        assert!(rate_limiter.should_send_report("test-type", start + min * 60));
        assert!(!rate_limiter.should_send_report("test-type", start + min * 61));
        assert!(!rate_limiter.should_send_report("test-type", start + min * 62));
        assert!(rate_limiter.should_send_report("test-type", start + min * 63));
    }

    #[test]
    fn test_rate_limiter_type_matching() {
        let mut rate_limiter = RateLimiter::new();
        let start = Instant::now();
        // Cause error error reports to be rate limited
        assert!(rate_limiter.should_send_report("componenta-network-error", start));
        assert!(!rate_limiter.should_send_report("componenta-network-error", start));
        // Other reports from the same component should also be rate limited
        assert!(!rate_limiter.should_send_report("componenta-database-error", start));
        // But not ones from other components
        assert!(rate_limiter.should_send_report("componentb-database-error", start));
        assert!(rate_limiter.should_send_report("componentaa-network-error", start));
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Reads nd-json (one LoginEntry JSON object per line) from stdin and reports
//! validation/fixup results.  Not meant to land; scratch tooling only.
//!
//! Usage:
//!   cargo run --bin validate_logins < entries.ndjson
//!   cat entries.ndjson | cargo run --bin validate_logins

use logins::{LoginEntry, ValidateAndFixup};
use std::collections::HashMap;
use std::io::{self, BufRead};

fn main() {
    let stdin = io::stdin();
    let mut count = 0usize;
    let mut ok = 0usize;
    let mut fixed = 0usize;
    let mut invalid_counts: HashMap<String, usize> = HashMap::new();

    for (i, line) in stdin.lock().lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[{i}] io error: {e}");
                continue;
            }
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let entry: LoginEntry = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[{i}] parse error: {e}");
                continue;
            }
        };

        match entry.maybe_fixup() {
            Ok(None) => {
                println!("[{i}] ok");
                ok += 1;
            }
            Ok(Some(fixed_entry)) => {
                println!("[{i}] fixed: {fixed_entry:?}");
                fixed += 1;
            }
            Err(e) => {
                println!("[{i}] invalid: {e}");
                *invalid_counts.entry(e.to_string()).or_default() += 1;
            }
        }

        count += 1;
    }

    let invalid_total: usize = invalid_counts.values().sum();
    let valid_total = ok + fixed;

    eprintln!("\n--- summary ({count} entries) ---");
    eprintln!("  ok:      {ok} ({:.2}%)", 100.0 * ok as f64 / count as f64);
    eprintln!(
        "  fixed:   {fixed} ({:.2}%)",
        100.0 * fixed as f64 / count as f64
    );
    eprintln!(
        "  valid:   {valid_total} ({:.2}%)",
        100.0 * valid_total as f64 / count as f64
    );
    eprintln!(
        "  invalid: {invalid_total} ({:.2}%)",
        100.0 * invalid_total as f64 / count as f64
    );
    if !invalid_counts.is_empty() {
        eprintln!("\n  breakdown:");
        let mut breakdown: Vec<_> = invalid_counts.iter().collect();
        breakdown.sort_by(|a, b| b.1.cmp(a.1));
        for (msg, n) in breakdown {
            eprintln!("    {n:>6}  {msg}");
        }
    }
}

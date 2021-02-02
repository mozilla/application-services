/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![warn(rust_2018_idioms)]

mod database;
mod matching;

use criterion::{criterion_group, criterion_main};
use database::{bench_match_url, bench_search_frecent};
use matching::bench_match_anywhere;

criterion_group!(bench_db, bench_search_frecent, bench_match_url);
criterion_group!(bench_mem, bench_match_anywhere);
criterion_main!(bench_db, bench_mem);

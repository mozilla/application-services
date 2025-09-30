/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use suggest::benchmarks::ingest;

fn main() {
    viaduct_hyper::init_backend_hyper().expect("Error initializing viaduct");
    ingest::print_debug_ingestion_sizes()
}

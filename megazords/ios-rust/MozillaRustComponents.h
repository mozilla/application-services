/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// This is the "umbrella header" for our combined Rust code library.
// It needs to import all of the individual headers.

#import "RustLogFFI.h"
#import "RustViaductFFI.h"
#import "autofillFFI.h"
#import "crashtestFFI.h"
#import "fxa_clientFFI.h"
#import "loginsFFI.h"
#import "nimbusFFI.h"
#import "pushFFI.h"
#import "tabsFFI.h"
// This is the uniffi-ed header
#import "placesFFI.h"
// This is the ffi header and will be deleted after uniffi
#import "RustPlacesAPI.h"
#import "glean.h"

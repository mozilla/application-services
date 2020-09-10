/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#import <UIKit/UIKit.h>

FOUNDATION_EXPORT double MegazordClientVersionNumber;
FOUNDATION_EXPORT const unsigned char MegazordClientVersionString[];

/**
 * FIXME: Glean has a `getGleanVersion` function that uses this constant.
 * That function is not actually used (and the version wrong anyway).
 * Because Glean is added as a submodule it's easier to change this occurence than it it to change the one of Glean,
 * for now.
 */
static double GleanVersionNumber = 0.0;

#import "RustFxAFFI.h"
#import "RustPasswordAPI.h"
#import "RustLogFFI.h"
#import "RustPlacesAPI.h"
#import "RustViaductFFI.h"
#import "GleanFfi.h"

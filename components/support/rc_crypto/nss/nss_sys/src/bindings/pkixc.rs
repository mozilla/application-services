/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::*;

extern "C" {
    pub fn VerifyCodeSigningCertificateChain(
        certificates: *mut *const u8, // Why mut? ``*mut *const u8`` --> ``const uint8_t **``
        certificateLengths: *const u16,
        numCertificates: size_t,
        secondsSinceEpoch: u64,
        rootSHA256Hash: *const u8,
        hostname: *const u8,
        hostnameLength: size_t,
        error: *mut PRErrorCode,
    ) -> bool;

    // bool VerifyCodeSigningCertificateChain(
    //     const uint8_t** certificates,
    //     const uint16_t* certificateLengths,
    //     size_t numCertificates,
    //     uint64_t secondsSinceEpoch,
    //     const uint8_t* rootSHA256Hash,
    //     const uint8_t* hostname,
    //     size_t hostnameLength,
    //     /* out */ PRErrorCode* error);
}

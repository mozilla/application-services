/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::convert::TryFrom;

use crate::error::*;
use crate::util::ensure_nss_initialized;

use nss_sys::PRErrorCode;

// NSS error codes.
// https://searchfox.org/mozilla-central/source/security/nss/lib/util/secerr.h#29
// https://searchfox.org/mozilla-central/source/security/nss/lib/mozpkix/include/pkix/Result.h
const SEC_ERROR_UNKNOWN_ISSUER: i32 = -8179; // -8192 + 13
const SEC_ERROR_EXPIRED_ISSUER_CERTIFICATE: i32 = -8162; // -8192 + 30
const SEC_ERROR_SUBJECT_MISMATCH: i32 = -12276; // ???
const SEC_ERROR_EXPIRED_CERTIFICATE: i32 = -16378; // ???

const ROOT_HASH_LENGTH: usize = 32;

pub fn verify_code_signing_certificate_chain(
    certificates: Vec<&[u8]>,
    seconds_since_epoch: u64,
    root_sha256_hash: &[u8],
    hostname: &str,
) -> Result<()> {
    ensure_nss_initialized();

    let mut cert_lens: Vec<u16> = vec![];
    for certificate in &certificates {
        match u16::try_from(certificate.len()) {
            Ok(v) => cert_lens.push(v),
            Err(e) => {
                return Err(ErrorKind::InputError(format!(
                    "certificate length is more than 65536 bytes: {}",
                    e
                ))
                .into());
            }
        }
    }

    // I cannot figure out how to get rid of `mut` here, because of
    // ``const uint8_t** certificates`` param in nss_sys.
    let mut p_certificates: Vec<_> = certificates.iter().map(|c| c.as_ptr()).collect();

    if root_sha256_hash.len() != ROOT_HASH_LENGTH {
        return Err(ErrorKind::InputError(format!(
            "root hash contains {} bytes instead of {}",
            root_sha256_hash.len(),
            ROOT_HASH_LENGTH
        ))
        .into());
    }

    let mut out: PRErrorCode = 0;

    let result = unsafe {
        nss_sys::VerifyCodeSigningCertificateChain(
            p_certificates.as_mut_ptr(),
            cert_lens.as_ptr(),
            certificates.len(),
            seconds_since_epoch,
            root_sha256_hash.as_ptr(),
            hostname.as_ptr(),
            hostname.len(),
            &mut out,
        )
    };

    if !result {
        let kind = match out {
            SEC_ERROR_UNKNOWN_ISSUER => ErrorKind::CertificateIssuerError,
            SEC_ERROR_EXPIRED_ISSUER_CERTIFICATE => ErrorKind::CertificateExpiredError,
            SEC_ERROR_EXPIRED_CERTIFICATE => ErrorKind::CertificateExpiredError,
            SEC_ERROR_SUBJECT_MISMATCH => ErrorKind::CertificateSubjectError,
            _ => ErrorKind::NSSError(out, "invalid chain of trust".into()),
        };
        return Err(kind.into());
    }

    Ok(())
}

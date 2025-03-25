/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::util::assert_nss_initialized;

use nss_sys::PRErrorCode;

// NSS error codes.
// https://searchfox.org/mozilla-central/rev/352b525/security/nss/lib/util/secerr.h#29
const SEC_ERROR_BASE: i32 = -0x2000; // -8192
const SEC_ERROR_EXPIRED_CERTIFICATE: i32 = SEC_ERROR_BASE + 11;
const SEC_ERROR_UNKNOWN_ISSUER: i32 = SEC_ERROR_BASE + 13;
const SEC_ERROR_EXPIRED_ISSUER_CERTIFICATE: i32 = SEC_ERROR_BASE + 30;

// SSL error codes.
// https://searchfox.org/mozilla-central/rev/352b525/security/nss/lib/ssl/sslerr.h#42
const SSL_ERROR_BASE: i32 = -0x3000; // -12288
const SSL_ERROR_BAD_CERT_DOMAIN: i32 = SSL_ERROR_BASE + 12;

// PKIX error codes.
// https://searchfox.org/mozilla-central/rev/352b525/security/nss/lib/mozpkix/include/pkix/pkixnss.h#81
const PKIX_ERROR_BASE: i32 = -0x4000; // -16384
const PKIX_ERROR_NOT_YET_VALID_CERTIFICATE: i32 = PKIX_ERROR_BASE + 5;
const PKIX_ERROR_NOT_YET_VALID_ISSUER_CERTIFICATE: i32 = PKIX_ERROR_BASE + 6;

const ROOT_HASH_LENGTH: usize = 32;

pub fn verify_code_signing_certificate_chain(
    certificates: Vec<&[u8]>,
    seconds_since_epoch: u64,
    root_sha256_hash: &[u8],
    hostname: &str,
) -> Result<()> {
    assert_nss_initialized();

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
            p_certificates.as_mut_ptr(), // Ideally the exposed API should not require mutability here.
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
            SEC_ERROR_EXPIRED_CERTIFICATE => ErrorKind::CertificateValidityError,
            SEC_ERROR_EXPIRED_ISSUER_CERTIFICATE => ErrorKind::CertificateValidityError,
            PKIX_ERROR_NOT_YET_VALID_CERTIFICATE => ErrorKind::CertificateValidityError,
            PKIX_ERROR_NOT_YET_VALID_ISSUER_CERTIFICATE => ErrorKind::CertificateValidityError,
            SSL_ERROR_BAD_CERT_DOMAIN => ErrorKind::CertificateSubjectError,
            _ => {
                let msg = "invalid chain of trust".to_string();
                if SSL_ERROR_BASE < out && out < SSL_ERROR_BASE + 1000 {
                    ErrorKind::SSLError(out, msg)
                } else if PKIX_ERROR_BASE < out && out < PKIX_ERROR_BASE + 1000 {
                    ErrorKind::PKIXError(out, msg)
                } else {
                    ErrorKind::NSSError(out, msg)
                }
            }
        };
        return Err(kind.into());
    }

    Ok(())
}

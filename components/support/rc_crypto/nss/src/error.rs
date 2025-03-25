/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("NSS error: {0} {1}")]
    NSSError(i32, String),
    #[error("SSL error: {0} {1}")]
    SSLError(i32, String),
    #[error("PKIX error: {0} {1}")]
    PKIXError(i32, String),
    #[error("Input or format error: {0}")]
    InputError(String),
    #[error("Internal crypto error")]
    InternalError,
    #[error("invalid key length")]
    InvalidKeyLength,
    #[error("Interior nul byte was found")]
    NulError,
    #[error("Conversion error: {0}")]
    ConversionError(#[from] std::num::TryFromIntError),
    #[error("Base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Certificate issuer does not match")]
    CertificateIssuerError,
    #[error("Certificate subject does not match")]
    CertificateSubjectError,
    #[error("Certificate not yet valid or expired")]
    CertificateValidityError,
}

error_support::define_error! {
    ErrorKind {
        (Base64Decode, base64::DecodeError),
        (ConversionError, std::num::TryFromIntError),
    }
}

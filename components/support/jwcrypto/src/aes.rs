/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Wrappers we use around rc_crypto's AES implementation. Specifically,
// "enc=A256GCM" from RFC7518, Section 4.7 - for all the gory details, see
// https://tools.ietf.org/html/rfc7518#section-4.7.

use crate::{
    error::{JwCryptoError, Result},
    CompactJwe, EncryptionAlgorithm, JweHeader,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use crypto_traits::aead::AeadAlgorithm;

/// Does the AES-encrypt heavy-lifting for the schemes supported by this crate.
pub(crate) fn aes_gcm_encrypt(
    data: &[u8],
    protected_header: JweHeader,
    content_encryption_key: &[u8],
) -> Result<CompactJwe> {
    let crypto = crypto_traits::get_cryptographer()?;
    assert_eq!(protected_header.enc, EncryptionAlgorithm::A256GCM);
    let additional_data = serde_json::to_string(&protected_header)?;
    let additional_data = URL_SAFE_NO_PAD.encode(additional_data.as_bytes());
    let additional_data = additional_data.as_bytes();
    // Note that RFC7518 specifies an IV of 96 bits == 12 bytes - which means
    // that a random IV generally isn't safe with AESGCM due to the risk of
    // collisions in this many bits. However, for the use-cases supported by
    // this crate, the keys are either ephemeral, or the number of encryptions
    // for the same key is expected to be low enough to not collide in
    // practice.
    let mut iv: Vec<u8> = vec![0; 12];
    crypto.rand(&mut iv)?;
    let mut encrypted = crypto.seal(
        AeadAlgorithm::Aes256Gcm,
        content_encryption_key,
        Some(&iv),
        data,
        additional_data,
    )?;

    let tag_idx = encrypted.len() - AeadAlgorithm::Aes256Gcm.tag_len();
    let auth_tag = encrypted.split_off(tag_idx);
    let ciphertext = encrypted;

    CompactJwe::new(
        Some(protected_header),
        None,
        Some(iv),
        ciphertext,
        Some(auth_tag),
    )
}

/// Does the AES-decrypt heavy-lifting for the schemes supported by this crate
pub(crate) fn aes_gcm_decrypt(jwe: &CompactJwe, content_encryption_key: &[u8]) -> Result<String> {
    let protected_header = jwe
        .protected_header()?
        .ok_or(JwCryptoError::IllegalState("missing protected_header"))?;
    assert_eq!(protected_header.enc, EncryptionAlgorithm::A256GCM);
    let auth_tag = jwe
        .auth_tag()?
        .ok_or(JwCryptoError::IllegalState("auth_tag must be present."))?;
    if auth_tag.len() != AeadAlgorithm::Aes256Gcm.tag_len() {
        return Err(JwCryptoError::IllegalState(
            "The auth tag length is incorrect",
        ));
    }
    let iv = jwe
        .iv()?
        .ok_or(JwCryptoError::IllegalState("iv must be present."))?;
    let ciphertext_and_tag: Vec<u8> = [jwe.ciphertext()?, auth_tag].concat();
    let aad = jwe.protected_header_raw().as_bytes();
    let crypto = crypto_traits::get_cryptographer()?;
    let plaintext = crypto.open(
        AeadAlgorithm::Aes256Gcm,
        content_encryption_key,
        Some(&iv),
        &ciphertext_and_tag,
        aad,
    )?;
    Ok(String::from_utf8(plaintext.to_vec())?)
}

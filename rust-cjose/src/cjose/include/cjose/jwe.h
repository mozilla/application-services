/*
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */

/**
 * \file  jwe.h
 * \brief Functions and data structures for interacting with
 *        JSON Web Encryption (JWE) objects.
 *
 */

#ifndef CJOSE_JWE_H
#define CJOSE_JWE_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>
#include "header.h"
#include "error.h"
#include "jwk.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Supplemental structure to represent JWE recipients
 */
typedef struct {
    /** Key to use for this recipient */
    const cjose_jwk_t * jwk;
    /** Additional unprotected header for this recipient */
    cjose_header_t *unprotected_header;
} cjose_jwe_recipient_t;

/**
 * An instance of a JWE object.
 */
typedef struct _cjose_jwe_int cjose_jwe_t;

typedef const cjose_jwk_t *(*cjose_key_locator)(cjose_jwe_t *jwe, cjose_header_t *hdr, void *);

/**
 * Creates a new JWE by encrypting the given plaintext within the given header
 * and JWK.
 *
 * If the header provided indicates an algorithm requiring an asymmetric key
 * (e.g. RSA-OAEP), the provided JWK must be asymmetric (e.g. RSA or EC).
 *
 * If the header provided indicates an algorithm requiring a symmetric key
 * (e.g. (dir), the provided JWK must be symmetric (e.g. oct).
 *
 * \param jwk [in] the key to use for encrypting the JWE.
 * \param protected_header [in] additional header values to include in the JWE protected header.
 * \param plaintext [in] the plaintext to be encrypted in the JWE payload.
 * \param plaintext_len [in] the length of the plaintext.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns a newly generated JWE with the given plaintext as the payload.
 */
cjose_jwe_t *
cjose_jwe_encrypt(const cjose_jwk_t *jwk, cjose_header_t *header, const uint8_t *plaintext, size_t plaintext_len, cjose_err *err);

/**
 * Creates a new JWE by encrypting the given plaintext with multiple keys.
 * \see ::cjose_jwe_encrypt for key requirements.
 * \param recipients [in] array of recipient objects. Each element must have the
 *        key of the recipient, and may have optional (not NULL) unprotected header.
 *        Unprotected header is retained by this function, and can be safely released by the
 *        caller if no longer needed. The key is only used within the scope of this function.
 * \param recipient_count effective length of the recipients array, specifying how many
 *        recipients there is.
 * \param protected_header [in] additional header values to include in the JWE protected header. The header
 *        is retained by JWE and should be released by the caller if no longer needed.
 * \param unprotected_header [in] additional header values to include in the shared JWE unprotected header,
 *        can be NULL. The header is retained by JWE and should be released by the caller if no longer needed.
 * \param plaintext [in] the plaintext to be encrypted in the JWE payload.
 * \param plaintext_len [in] the length of the plaintext.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns a newly generated JWE with the given plaintext as the payload.
 */
cjose_jwe_t *cjose_jwe_encrypt_multi(const cjose_jwe_recipient_t * recipients,
                                    size_t recipient_count,
                                    cjose_header_t *protected_header,
                                    cjose_header_t *shared_unprotected_header,
                                    const uint8_t *plaintext,
                                    size_t plaintext_len,
                                    cjose_err *err);

/**
 * Creates a compact serialization of the given JWE object.
 *
 * \param jwe [in] The JWE object to be serialized.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns A pointer to a compact serialization of this JWE.  Note
 *        the returned string pointer is owned by the caller, the caller
 *        must free it directly when no longer needed, or the memory will be
 *        leaked.
 */
char *cjose_jwe_export(cjose_jwe_t *jwe, cjose_err *err);

/**
 * Creates a JSON serialization of the given JWE object.
 *
 * \param jwe [in] The JWE object to be serialized.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns A pointer to a JSON serialization of this JWE.  Note
 *        the returned string pointer is owned by the caller, the caller
 *        must free it directly when no longer needed, or the memory will be
 *        leaked.
 */
char *cjose_jwe_export_json(cjose_jwe_t *jwe, cjose_err *err);

/**
 * Creates a new JWE object from the given JWE compact serialization.
 *
 * Note the current implementation only recognizes the JWE compact serialization
 * format.
 *
 * \param compact [in] a JWE in serialized form.
 * \param compact_len [in] the length of the compact serialization.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns a newly generated JWE object from the given JWE serialization.
 */
cjose_jwe_t *cjose_jwe_import(const char *compact, size_t compact_len, cjose_err *err);

/**
 * Creates a new JWE object from the given JWE compact serialization.
 *
 * Note the current implementation only recognizes the JWE compact serialization
 * format.
 *
 * \param json [in] a JWE in a JSON serialized form.
 * \param json_len [in] the length of the serialization.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns a newly generated JWE object from the given JWE JSON serialization.
 */
cjose_jwe_t *cjose_jwe_import_json(const char *json, size_t json_len, cjose_err *err);

/**
 * Decrypts the JWE object using the given JWK.  Returns the plaintext data of
 * the JWE payload.
 *
 * \param jwe [in] the JWE object to decrypt.
 * \param jwk [in] the key to use for decrypting.
 * \param content_len [out] The number of bytes in the returned buffer.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The decrypted content.  Note the caller is responsible for free'ing
 *        this buffer when no longer in use.  Failure to do so will result in
 *        a memory leak.
 */
uint8_t *cjose_jwe_decrypt(cjose_jwe_t *jwe, const cjose_jwk_t *jwk, size_t *content_len, cjose_err *err);

/**
 * Decrypts the JWE object using one or more provided JWKs. Returns the plaintext data
 * of the JWE payload. The key to be used for decryption must be provided by the specified call back.
 * The call back will be invoked for each recipient information in the JWE.
 * If no key is available for a particular recipient information, `NULL` must be returned.
 * More than one key can be returned, decryption is considered successful if the content
 * decrypts and validates against all returned non-NULL keys, and at least one key was attempted.
 *
 * \param jwe [in] the JWE object to decrypt.
 * \param jwk [in] key_locator callback for finding keys
 * \param data [in] custom data argument that is passed to the key locator callback.
 * \param content_len [out] The number of bytes in the returned buffer.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The decrypted content. Note the caller is responsible for free'ing
 *        this buffer when no longer in use.  Failure to do so will result in
 *        a memory leak.
 */
uint8_t *cjose_jwe_decrypt_multi(cjose_jwe_t *jwe, cjose_key_locator key_locator, void *data, size_t *content_len, cjose_err *err);

/**
 * Returns the protected header of the JWE object.
 *
 * **NOTE:** The returned header is still owned by the JWE object. Users must
 * call `cjose_header_retain()` if it is expected to be valid after the
 * owning `cjose_jwe_t` is released.
 *
 * \param jwe [in] the JWE object for which the protected header is requested.
 * \returns the (parsed) protected header
 */
cjose_header_t *cjose_jwe_get_protected(cjose_jwe_t *jwe);

/**
 * Releases the given JWE object.
 *
 * \param jwe the JWE to be released.  If null, this is a no-op.
 */
void cjose_jwe_release(cjose_jwe_t *jwe);

#ifdef __cplusplus
}
#endif

#endif // CJOSE_JWE_H

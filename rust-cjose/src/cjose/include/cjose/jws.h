/*
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */

/**
 * \file  jws.h
 * \brief Functions and data structures for interacting with
 *        JSON Web Signature (JWS) objects.
 *
 */

#ifndef CJOSE_JWS_H
#define CJOSE_JWS_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>
#include "header.h"
#include "jwk.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * An instance of a JWS object.
 */
typedef struct _cjose_jws_int cjose_jws_t;

/**
 * Creates a new JWS by signing the given plaintext within the given header
 * and JWK.
 *
 * \param jwk [in] the key to use for signing the JWS.
 * \param protected_header [in] header values to include in the JWS header.
 * \param plaintext [in] the plaintext to be signed as the JWS payload.
 * \param plaintext_len [in] the length of the plaintext.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns a newly generated JWS with the given plaintext as the payload.
 */
cjose_jws_t *cjose_jws_sign(
    const cjose_jwk_t *jwk, cjose_header_t *protected_header, const uint8_t *plaintext, size_t plaintext_len, cjose_err *err);

/**
 * Creates a serialization of the given JWS object.
 *
 * Note the current implementation only supports serialization to the JWS
 * compact format.
 *
 * \param jws [in] the JWS object to be serialized.
 * \param ser [out] pointer to a compact serialization of this JWS.  Note
 *        the returned string pointer is owned by the JWS, the caller should
 *        not attempt to free it directly, and note that it will be freed
 *        automatically when the JWS itself is released.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns true if the serialization is successfully returned.
 */
bool cjose_jws_export(cjose_jws_t *jws, const char **ser, cjose_err *err);

/**
 * Creates a new JWS object from the given JWS compact serialization.
 *
 * Note the current implementation only recognizes the JWS compact serialization
 * format.
 *
 * \param compact [in] a JWS in serialized form.
 * \param compact_len [in] the length of the compact serialization.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns a newly generated JWS object from the given JWS serialization.
 */
cjose_jws_t *cjose_jws_import(const char *compact, size_t compact_len, cjose_err *err);

/**
 * Verifies the JWS object using the given JWK.
 *
 * \param jws [in] the JWS object to verify.
 * \param jwk [in] the key to use for verification.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns true if verification was sucecssful.
 */
bool cjose_jws_verify(cjose_jws_t *jws, const cjose_jwk_t *jwk, cjose_err *err);

/**
 * Returns the plaintext data of the JWS payload.
 *
 * \param jws [in] the JWS object for which the plaintext is requested.
 * \param plaintext [out] pointer to the plaintext of this JWS.  Note
 *        the returned buffer is owned by the JWS, the caller should
 *        not attempt to free it directly, and note that it will be freed
 *        automatically when the JWS itself is released.
 * \param plaintext_len [out] number of bytes of plaintext in the returned
 *        plaintext buffer.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns true if the plaintext is sucessfully returned.
 */
bool cjose_jws_get_plaintext(const cjose_jws_t *jws, uint8_t **plaintext, size_t *plaintext_len, cjose_err *err);

/**
 * Returns the protected header of the JWS payload.
 *
 * **NOTE:** The returned header is still owned by the JWS object. Users must
 * call `cjose_header_retain()` if it is expected to be valid after the
 * owning `cjose_jws_t` is released.
 *
 * \param jws [in] the JWS object for which the protected header is requested.
 * \returns the (parsed) protected header
 */
cjose_header_t *cjose_jws_get_protected(cjose_jws_t *jws);

/**
 * Releases the given JWS object.
 *
 * \param jws the JWS to be released.  If null, this is a no-op.
 */
void cjose_jws_release(cjose_jws_t *jws);

#ifdef __cplusplus
}
#endif

#endif // CJOSE_JWS_H

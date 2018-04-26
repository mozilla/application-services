/*
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */
/**
 * \file
 * \brief
 * Functions and data structures for interacting with JSON Web Key (JWK) objects.
 *
 */

#ifndef CJOSE_JWK_H
#define CJOSE_JWK_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>
#include <openssl/obj_mac.h>
#include "cjose/error.h"
#include "cjose/header.h"

#ifdef __cplusplus
extern "C" {
#endif

/** Enumeration of supported JSON Web Key (JWK) types */
typedef enum {
    /** RSA Public (or Private) Key */
    CJOSE_JWK_KTY_RSA = 1,
    /** Elliptic Curve Public (or Private) Key */
    CJOSE_JWK_KTY_EC,
    /** Octet String (Symmetric) Key */
    CJOSE_JWK_KTY_OCT
} cjose_jwk_kty_t;

/**
 * Retrieves the string name for the given KTY enumeration.
 *
 * \param kty The JWK key type
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The string name for <tt>kty</tt>
 */
const char *cjose_jwk_name_for_kty(cjose_jwk_kty_t kty, cjose_err *err);

/** An instance of a JWK object. */
typedef struct _cjose_jwk_int cjose_jwk_t;

/**
 * Retains a JWK object.  The caller MUST call cjose_jwk_release() once the
 * JWK object is no longer in use, or the program will leak memory.
 *
 * \param jwk The JWK object to retain
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The retained JWK object
 */
cjose_jwk_t *cjose_jwk_retain(cjose_jwk_t *jwk, cjose_err *err);

/**
 * Releases a JWK object. For a newly-created key where cjose_jwk_retain() has
 * not been called, the object is destroyed and its resources are released.
 *
 * If cjose_jwk_retain() has been called on the given JWK object, an internal
 * count is decremented but no other resources are released.
 *
 * \param jwk The JWK object to release
 * \returns <tt>true</tt> if the released JWK is still valid, or <tt>false</tt>
 * if the JWK object's resources and memory have been freed.
 */
bool cjose_jwk_release(cjose_jwk_t *jwk);

/**
 * Retrieves the key type for the given JWK object.
 *
 * \param jwk The JWK object
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns the known key type
 */
cjose_jwk_kty_t cjose_jwk_get_kty(const cjose_jwk_t *jwk, cjose_err *err);

/**
 * Retrieves the keysize of this JWK (in bits).
 *
 * \param jwk The JWK to retrieve the keysize of
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The keysize for this JWK, in bits
 */
size_t cjose_jwk_get_keysize(const cjose_jwk_t *jwk, cjose_err *err);

/**
 * Retrieves the raw key data for this JWK.
 *
 * \b WARNING: this is the raw data specific to the key type, and could
 * contain private key material.
 * \b NOTE: This key data will be released when the key is released.
 *
 * \param jwk The JWK to retrieve key data from
 * \returns The key data specific to the type of key
 */
void *cjose_jwk_get_keydata(const cjose_jwk_t *jwk, cjose_err *err);

/**
 * Retrieves the key id for the given JWK object.  The string returned by
 * this call belongs to the JWK, caller should not attempt to free it.
 *
 * \param jwk The JWK object
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns the current kid, or NULL if not set.
 */
const char *cjose_jwk_get_kid(const cjose_jwk_t *jwk, cjose_err *err);

/**
 * Sets a new value for the kid attribute of the JWK.  The string will be
 * copied to an internal buffer of the JWK and released when the JWK object
 * is released.
 *
 * \param jwk The JWK object
 * \param kid The new kid value
 * \param len The length of the kid string in bytes
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns True if successful, false otherwise.
 */
bool cjose_jwk_set_kid(cjose_jwk_t *jwk, const char *kid, size_t len, cjose_err *err);

/**
 * Serializes the given JWK to a JSON string.
 *
 * \b NOTE: The returned needs to be freed by the caller.
 *
 * \param jwk The JWK to serialize.
 * \param priv <tt>true</tt> to include private/secret fields
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The JSON string representation of <tt>jwk</tt>
 */
char *cjose_jwk_to_json(const cjose_jwk_t *jwk, bool priv, cjose_err *err);

/** Key specification for RSA JWK objects. */
typedef struct
{
    /** Public exponent */
    uint8_t *e;
    /** Length of <tt>e</tt> */
    size_t elen;
    /** Modulus */
    uint8_t *n;
    /** Length of <tt>n</tt> */
    size_t nlen;
    /** Private exponent */
    uint8_t *d;
    /** Length of <tt>d</tt> */
    size_t dlen;
    /** First prime */
    uint8_t *p;
    /** Length of <tt>p</tt> */
    size_t plen;
    /** Second prime */
    uint8_t *q;
    /** Length of <tt>q</tt> */
    size_t qlen;
    /** d (mod p - 1) */
    uint8_t *dp;
    /** Length of <tt>dp</tt> */
    size_t dplen;
    /** d (mod q - 1) */
    uint8_t *dq;
    /** Length of <tt>dq</tt> */
    size_t dqlen;
    /** coefficient */
    uint8_t *qi;
    /** Length of <tt>qi</tt> */
    size_t qilen;
} cjose_jwk_rsa_keyspec;

/**
 * Creates a new RSA JWK, using a secure random number generator.
 *
 * \b NOTE: The caller MUST call cjose_jwk_release() to release the JWK's
 * resources.
 *
 * \param size The keysize, in bits
 * \param e The public exponent
 * \param elen The length of <tt>e</tt>
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The generated symmetric JWK object.
 */
cjose_jwk_t *cjose_jwk_create_RSA_random(size_t keysize, const uint8_t *e, size_t elen, cjose_err *err);

/**
 * Creates a new RSA JWK, using the given raw value for the private
 * and/or public keys.
 *
 * \b NOTE: The caller MUST call cjose_jwk_release() to release the JWK's
 * resources.
 *
 * \b NOTE: This function makes a copy of all provided data; the caller
 * MUST free the memory for <tt>spec</tt> after calling this function.
 *
 * \param spec The specified RSA key properties
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The generated RSA JWK object
 */
cjose_jwk_t *cjose_jwk_create_RSA_spec(const cjose_jwk_rsa_keyspec *spec, cjose_err *err);

/** Enumeration of supported Elliptic-Curve types */
typedef enum {
    /** NIST P-256 Prime Curve (secp256r1) */
    CJOSE_JWK_EC_P_256 = NID_X9_62_prime256v1,
    /** NIST P-384 Prime Curve (secp384r1) */
    CJOSE_JWK_EC_P_384 = NID_secp384r1,
    /** NIST P-521 Prime Curve (secp521r1) */
    CJOSE_JWK_EC_P_521 = NID_secp521r1,
    /** Invalid Curve */
    CJOSE_JWK_EC_INVALID = -1
} cjose_jwk_ec_curve;

/** Key specification for Elliptic Curve JWK objects. */
typedef struct
{
    /** The elliptic curve */
    cjose_jwk_ec_curve crv;
    /** The private key */
    uint8_t *d;
    /** Length of <tt>d</tt> */
    size_t dlen;
    /** The public key's X coordinate */
    uint8_t *x;
    /** Length of <tt>x</tt> */
    size_t xlen;
    /** The public key's Y coordiate */
    uint8_t *y;
    /** Length of <tt>y</tt> */
    size_t ylen;
} cjose_jwk_ec_keyspec;

/**
 * Creates a new Elliptic-Curve JWK, using a secure random number generator.
 *
 * \b NOTE: The caller MUST call cjose_jwk_release() to release the JWK's
 * resources.
 *
 * \param crv The EC Curve to generate against
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The generated Elliptic Curve JWK object
 */
cjose_jwk_t *cjose_jwk_create_EC_random(cjose_jwk_ec_curve crv, cjose_err *err);

/**
 * Creates a new Elliptic-Curve JWK, using the given the raw values for
 * the private and/or public keys.
 *
 * \b NOTE: The caller MUST call cjose_jwk_release() to release the JWK's
 * resources.
 *
 * \b NOTE: This function makes a copy of all provided data; the caller
 * MUST free the memory for <tt>spec</tt> after calling this function.
 *
 * \param spec The specified Elliptic Curve key properties
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The generated Elliptic Curve JWK object
 */
cjose_jwk_t *cjose_jwk_create_EC_spec(const cjose_jwk_ec_keyspec *spec, cjose_err *err);

/**
 * Obtains the curve for the given (EC) JWK.
 *
 * \param jwk [in] The EC JWK to inspect
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The curve type
 */
const cjose_jwk_ec_curve cjose_jwk_EC_get_curve(const cjose_jwk_t *jwk, cjose_err *err);

/**
 * Creates a new symmetric octet JWK, using a secure random number generator.
 *
 * \b NOTE: The caller MUST call cjose_jwk_release() to release the JWK's
 * resources.
 *
 * \param size The keysize, in bits
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The generated symmetric JWK object.
 */
cjose_jwk_t *cjose_jwk_create_oct_random(size_t size, cjose_err *err);

/**
 * Creates a new symmetric oct JWK, using the given raw key data.
 *
 * \b NOTE: <tt>data</tt> is copied, so the caller MUST free its memory
 * after calling this function.
 *
 * \b NOTE: The caller MUST call cjose_jwk_release() to release the JWK's
 * resources.
 *
 * \param data The key value.
 * \param len The length of <tt>data</tt>
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns The symmetric JWK object for the given raw key data.
 */
cjose_jwk_t *cjose_jwk_create_oct_spec(const uint8_t *data, size_t len, cjose_err *err);

/**
 * Instantiates a new JWK given a JSON document representation conforming
 * to JSON Web Key (JWK) IETF ID draft-ietf-jose-json-web-key.
 *
 * \b NOTE: A successful call returns a new cjose_jwk_t object. It is the
 * caller's responsibility to call cjose_jwk_release() to release the JWK when
 * it is no longer needed.  Failure to do so will result in a memory leak.
 *
 * \param json A JSON document conforming to the Jose JWK specification.
 * \param len The length of the given JSON document.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns A JWK object corresponding to the given JSON document.  In
 *        the event the given JSON document cannot be parsed, or is
 *        otherwise an invalid JWK representation, this will return NULL.
 */
cjose_jwk_t *cjose_jwk_import(const char *json, size_t len, cjose_err *err);

/**
 * Instantiates a new JWK given a JSON object conforming to JSON Web Key (JWK)
 * IETF RFC 7518.
 *
 * \b NOTE: A successful call returns a new cjose_jwk_t object. It is the
 * caller's responsibility to call cjose_jwk_release() to release the JWK when
 * it is no longer needed.  Failure to do so will result in a memory leak.
 *
 * \param json A JSON document conforming to the Jose JWK specification.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns A JWK object corresponding to the given JSON document.  In
 *        the event the given JSON object is an invalid JWK representation, this
 *        will return NULL.
 */
cjose_jwk_t *cjose_jwk_import_json(cjose_header_t *json, cjose_err *err);

/**
 * Computes an ECDH ephemeral key as an HKDF hash of the derived shared
 * secret from a local EC key-pair and a peer's EC public key.  The result is
 * returned in the form of a new JWK of type oct.
 *
 * Note: on successful return of a jwk_ecdh_ephemeral_key, the caller becomes
 * responsible for releasing that JWK wuth the cjose_jwk_release() command.
 *
 * \param jwk_self [in] The caller's own EC key pair.
 * \param jwk_peer [in] The peer's EC public key.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 * \returns A new JWK representing the ephemeral key, or NULL in the event of
 *        and error.
 */
cjose_jwk_t *cjose_jwk_derive_ecdh_ephemeral_key(const cjose_jwk_t *jwk_self, const cjose_jwk_t *jwk_peer, cjose_err *err);

/**
 Deprecated.  Alias for cjose_jwk_derive_ecdh_ephemeral_key.
*/
cjose_jwk_t *cjose_jwk_derive_ecdh_secret(const cjose_jwk_t *jwk_self, const cjose_jwk_t *jwk_peer, cjose_err *err);

#ifdef __cplusplus
}
#endif

#endif // CJOSE_JWK_H

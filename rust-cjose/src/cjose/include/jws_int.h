/*!
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */

#ifndef SRC_JWS_INT_H
#define SRC_JWS_INT_H

#include <jansson.h>
#include "cjose/jwe.h"

// functions for building JWS parts
typedef struct _jws_fntable_int
{
    bool (*digest)(cjose_jws_t *jws, const cjose_jwk_t *jwk, cjose_err *err);

    bool (*sign)(cjose_jws_t *jws, const cjose_jwk_t *jwk, cjose_err *err);

    bool (*verify)(cjose_jws_t *jws, const cjose_jwk_t *jwk, cjose_err *err);

} jws_fntable;

// JWS object
struct _cjose_jws_int
{
    json_t *hdr; // header JSON object

    char *hdr_b64u; // serialized and base64url encoded header
    size_t hdr_b64u_len;

    uint8_t *dat; // payload data
    size_t dat_len;

    char *dat_b64u; // base64url encoded payload data
    size_t dat_b64u_len;

    uint8_t *dig; // digest of signing input value
    size_t dig_len;

    uint8_t *sig; // signature
    size_t sig_len;

    char *sig_b64u; // base64url encoded signature
    size_t sig_b64u_len;

    char *cser; // compact serialization
    size_t cser_len;

    jws_fntable fns; // functions for building JWS parts
};

#endif // SRC_JWS_INT_H

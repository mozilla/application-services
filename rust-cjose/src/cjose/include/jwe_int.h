/*!
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */

#ifndef SRC_JWE_INT_H
#define SRC_JWE_INT_H

#include <jansson.h>
#include "cjose/jwe.h"

// JWE part
typedef struct _cjose_jwe_part_int
{
    uint8_t *raw;
    size_t raw_len;

    char *b64u;
    size_t b64u_len;
} _jwe_part_t;

typedef struct _cjose_jwe_recipient _jwe_int_recipient_t;

// functions for building JWE parts
typedef struct _jwe_rec_fntable_int
{
    bool (*encrypt_ek)(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

    bool (*decrypt_ek)(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

} jwe_rec_fntable;

typedef struct _jwe_fntable_int
{

    bool (*set_cek)(cjose_jwe_t *jwe, const cjose_jwk_t *jwk, bool random, cjose_err *err);

    bool (*set_iv)(cjose_jwe_t *jwe, cjose_err *err);

    bool (*encrypt_dat)(cjose_jwe_t *jwe, const uint8_t *plaintext, size_t plaintext_len, cjose_err *err);

    bool (*decrypt_dat)(cjose_jwe_t *jwe, cjose_err *err);

} jwe_fntable;

struct _cjose_jwe_recipient
{

    json_t *unprotected;                /* unprotected headers */
    _jwe_part_t enc_key;                /* encrypted key */
    jwe_rec_fntable fns;                // functions for building JWE parts
};

// JWE object
struct _cjose_jwe_int
{
    json_t *hdr;        // header JSON object
    json_t *shared_hdr; // shared header JSON object

    // _jwe_part_t part[5]; // the 5 compact JWE parts

    _jwe_part_t enc_header;
    _jwe_part_t enc_iv;
    _jwe_part_t enc_ct;
    _jwe_part_t enc_auth_tag;

    jwe_fntable fns;

    uint8_t *cek; // content-encryption key
    size_t cek_len;

    uint8_t *dat; // decrypted data
    size_t dat_len;

    size_t to_count; // recipients count.
    _jwe_int_recipient_t *to;
};

#endif // SRC_JWE_INT_H

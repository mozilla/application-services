/*!
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */

#include <cjose/base64.h>
#include <cjose/header.h>
#include <cjose/jwe.h>
#include <cjose/util.h>

#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <openssl/rand.h>
#include <openssl/rsa.h>
#include <openssl/evp.h>
#include <openssl/aes.h>
#include <openssl/hmac.h>

#include "include/concatkdf_int.h"
#include "include/header_int.h"
#include "include/jwk_int.h"
#include "include/jwe_int.h"
#include "include/util_int.h"



////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_set_cek_a256gcm(cjose_jwe_t *jwe, const cjose_jwk_t *jwk, bool random, cjose_err *err);

static bool _cjose_jwe_set_cek_aes_cbc(cjose_jwe_t *jwe, const cjose_jwk_t *jwk, bool random, cjose_err *err);

static bool
_cjose_jwe_encrypt_ek_dir(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

static bool
_cjose_jwe_decrypt_ek_dir(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

static bool
_cjose_jwe_encrypt_ek_aes_kw(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

static bool
_cjose_jwe_decrypt_ek_aes_kw(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

static bool
_cjose_jwe_encrypt_ek_rsa_oaep(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

static bool
_cjose_jwe_decrypt_ek_rsa_oaep(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

static bool
_cjose_jwe_encrypt_ek_rsa1_5(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

static bool
_cjose_jwe_decrypt_ek_rsa1_5(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

static bool
_cjose_jwe_encrypt_ek_ecdh_es(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

static bool
_cjose_jwe_decrypt_ek_ecdh_es(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err);

static bool _cjose_jwe_set_iv_a256gcm(cjose_jwe_t *jwe, cjose_err *err);

static bool _cjose_jwe_set_iv_aes_cbc(cjose_jwe_t *jwe, cjose_err *err);

static bool _cjose_jwe_encrypt_dat_a256gcm(cjose_jwe_t *jwe, const uint8_t *plaintext, size_t plaintext_len, cjose_err *err);

static bool _cjose_jwe_encrypt_dat_aes_cbc(cjose_jwe_t *jwe, const uint8_t *plaintext, size_t plaintext_len, cjose_err *err);

static bool _cjose_jwe_decrypt_dat_a256gcm(cjose_jwe_t *jwe, cjose_err *err);

static bool _cjose_jwe_decrypt_dat_aes_cbc(cjose_jwe_t *jwe, cjose_err *err);

static void _cjose_release_cek(uint8_t **cek, size_t cek_len)
{

    if (NULL == *cek)
    {
        return;
    }

    memset(*cek, 0, cek_len);
    cjose_get_dealloc()(*cek);
    *cek = 0;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_empty_json(json_t *arg)
{

    return (NULL == arg || json_is_null(arg) || (json_is_object(arg) && NULL == json_object_iter_key(arg)));
}

////////////////////////////////////////////////////////////////////////////////
static void _cjose_dealloc_part(struct _cjose_jwe_part_int *part)
{

    cjose_get_dealloc()(part->raw);
    cjose_get_dealloc()(part->b64u);
}

static json_t *_cjose_parse_json_object(const char *str, size_t len, cjose_err *err)
{

    // unfortunately, it's not possible to tell whether the error is due
    // to syntax, or memory shortage. See https://github.com/akheron/jansson/issues/352

    json_error_t j_err;
    json_t *json = json_loadb(str, len, 0, &j_err);
    if (NULL == json || !json_is_object(json))
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        json_decref(json);
        return NULL;
    }

    return json;
}

static inline bool _cjose_convert_part(struct _cjose_jwe_part_int *part, cjose_err *err)
{

    if ((NULL == part->b64u)
        && (!cjose_base64url_encode((const uint8_t *)part->raw, part->raw_len, &part->b64u, &part->b64u_len, err)))
    {

        return false;
    }

    // dealloc the raw part, we will never need it again
    cjose_get_dealloc()(part->raw);
    part->raw = NULL;
    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_convert_to_base64(struct _cjose_jwe_int *jwe, cjose_err *err)
{

    if (!_cjose_convert_part(&jwe->enc_header, err) || !_cjose_convert_part(&jwe->enc_iv, err)
        || !_cjose_convert_part(&jwe->enc_iv, err) || !_cjose_convert_part(&jwe->enc_ct, err)
        || !_cjose_convert_part(&jwe->enc_auth_tag, err))
    {

        return false;
    }

    for (int i = 0; i < jwe->to_count; i++)
    {
        if (!_cjose_convert_part(&jwe->to[i].enc_key, err))
        {
            return false;
        }
    }

    return true;
}

////////////////////////////////////////////////////////////////////////////////
static size_t _keylen_from_enc(const char *alg)
{
    size_t keylen = 0;

    if (0 == strcmp(alg, CJOSE_HDR_ENC_A256GCM)) {
        keylen = 256;
    } else if (0 == strcmp(alg, CJOSE_HDR_ENC_A128CBC_HS256)) {
        keylen = 256;
    } else if (0 == strcmp(alg, CJOSE_HDR_ENC_A192CBC_HS384)) {
        keylen = 384;
    } else if (0 == strcmp(alg, CJOSE_HDR_ENC_A256CBC_HS512)) {
        keylen = 512;
    }

    return keylen;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_malloc(size_t bytes, bool random, uint8_t **buffer, cjose_err *err)
{
    *buffer = (uint8_t *)cjose_get_alloc()(bytes);
    if (NULL == *buffer)
    {
        CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
        return false;
    }
    if (random)
    {
        if (RAND_bytes((unsigned char *)*buffer, bytes) != 1)
        {
            cjose_get_dealloc()(*buffer);
            CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
            return false;
        }
    }
    else
    {
        memset(*buffer, 0, bytes);
    }
    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_build_hdr(cjose_jwe_t *jwe, cjose_err *err)
{
    // serialize the header
    char *hdr_str = json_dumps(jwe->hdr, JSON_ENCODE_ANY | JSON_PRESERVE_ORDER);
    if (NULL == hdr_str)
    {
        CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
        return false;
    }

    // copy the serialized header to JWE (hdr_str is owned by header object)
    size_t len = strlen(hdr_str);
    uint8_t *data = (uint8_t *)_cjose_strndup(hdr_str, len, err);
    if (!data)
    {
        cjose_get_dealloc()(hdr_str);
        return false;
    }

    jwe->enc_header.raw = data;
    jwe->enc_header.raw_len = len;
    cjose_get_dealloc()(hdr_str);

    return true;
}

static const char *_cjose_jwe_get_from_headers(cjose_header_t *protected_header,
                                               cjose_header_t *unprotected_header,
                                               cjose_header_t *personal_header,
                                               const char *key)
{

    // TODO: https://github.com/cisco/cjose/issues/52
    cjose_header_t *headers[] = { personal_header, unprotected_header, protected_header };

    for (int i = 0; i < 3; i++)
    {
        if (NULL == headers[i])
        {
            continue;
        }
        json_t *obj = json_object_get((json_t *)headers[i], key);
        if (NULL == obj)
        {
            continue;
        }
        const char *value = json_string_value(obj);
        if (NULL == value)
        {
            continue;
        }
        return value;
    }

    return NULL;
}

static bool _cjose_jwe_validate_enc(cjose_jwe_t *jwe, cjose_header_t *protected_header, cjose_err *err)
{

    const char *enc = cjose_header_get(protected_header, CJOSE_HDR_ENC, err);
    if (NULL == enc)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    if (strcmp(enc, CJOSE_HDR_ENC_A256GCM) == 0)
    {
        jwe->fns.set_cek = _cjose_jwe_set_cek_a256gcm;
        jwe->fns.set_iv = _cjose_jwe_set_iv_a256gcm;
        jwe->fns.encrypt_dat = _cjose_jwe_encrypt_dat_a256gcm;
        jwe->fns.decrypt_dat = _cjose_jwe_decrypt_dat_a256gcm;
    }
    if ((strcmp(enc, CJOSE_HDR_ENC_A128CBC_HS256) == 0) || (strcmp(enc, CJOSE_HDR_ENC_A192CBC_HS384) == 0)
        || (strcmp(enc, CJOSE_HDR_ENC_A256CBC_HS512) == 0))
    {
        jwe->fns.set_cek = _cjose_jwe_set_cek_aes_cbc;
        jwe->fns.set_iv = _cjose_jwe_set_iv_aes_cbc;
        jwe->fns.encrypt_dat = _cjose_jwe_encrypt_dat_aes_cbc;
        jwe->fns.decrypt_dat = _cjose_jwe_decrypt_dat_aes_cbc;
    }

    if (NULL == jwe->fns.set_cek || NULL == jwe->fns.set_iv || NULL == jwe->fns.encrypt_dat || NULL == jwe->fns.decrypt_dat)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_validate_alg(cjose_header_t *protected_header,
                                    cjose_header_t *unprotected_header,
                                    bool is_multiple,
                                    _jwe_int_recipient_t *recipient,
                                    cjose_err *err)
{

    const char *alg = _cjose_jwe_get_from_headers(protected_header, unprotected_header, (cjose_header_t *)recipient->unprotected,
                                                  CJOSE_HDR_ALG);

    if (NULL == alg)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // set JWE build functions based on header contents
    if (strcmp(alg, CJOSE_HDR_ALG_RSA_OAEP) == 0)
    {
        recipient->fns.encrypt_ek = _cjose_jwe_encrypt_ek_rsa_oaep;
        recipient->fns.decrypt_ek = _cjose_jwe_decrypt_ek_rsa_oaep;
    }
    if (strcmp(alg, CJOSE_HDR_ALG_RSA1_5) == 0)
    {
        recipient->fns.encrypt_ek = _cjose_jwe_encrypt_ek_rsa1_5;
        recipient->fns.decrypt_ek = _cjose_jwe_decrypt_ek_rsa1_5;
    }
    if (strcmp(alg, CJOSE_HDR_ALG_ECDH_ES) == 0)
    {
        if (is_multiple)
        {
            CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
            return false;
        }
        recipient->fns.encrypt_ek = _cjose_jwe_encrypt_ek_ecdh_es;
        recipient->fns.decrypt_ek = _cjose_jwe_decrypt_ek_ecdh_es;
    }
    if (strcmp(alg, CJOSE_HDR_ALG_DIR) == 0)
    {
        if (is_multiple)
        {
            CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
            return false;
        }
        recipient->fns.encrypt_ek = _cjose_jwe_encrypt_ek_dir;
        recipient->fns.decrypt_ek = _cjose_jwe_decrypt_ek_dir;
    }
    if ((strcmp(alg, CJOSE_HDR_ALG_A128KW) == 0) || (strcmp(alg, CJOSE_HDR_ALG_A192KW) == 0)
        || (strcmp(alg, CJOSE_HDR_ALG_A256KW) == 0))
    {
        recipient->fns.encrypt_ek = _cjose_jwe_encrypt_ek_aes_kw;
        recipient->fns.decrypt_ek = _cjose_jwe_decrypt_ek_aes_kw;
    }

    // ensure required builders have been assigned
    if (NULL == recipient->fns.encrypt_ek || NULL == recipient->fns.decrypt_ek)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_set_cek_a256gcm(cjose_jwe_t *jwe, const cjose_jwk_t *jwk, bool random, cjose_err *err)
{
    // 256 bits = 32 bytes
    static const size_t keysize = 32;

    if (NULL != jwe->cek)
    {
        return true;
    }

    // if no JWK is provided, generate a random key
    if (NULL == jwk)
    {
        _cjose_release_cek(&jwe->cek, jwe->cek_len);
        if (!_cjose_jwe_malloc(keysize, random, &jwe->cek, err))
        {
            return false;
        }
        jwe->cek_len = keysize;
    }
    else
    {
        // if a JWK is provided, it must be a symmetric key of correct size
        if (CJOSE_JWK_KTY_OCT != cjose_jwk_get_kty(jwk, err) || jwk->keysize != keysize * 8 || NULL == jwk->keydata)
        {
            CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
            return false;
        }

        // copy the key material directly from jwk to the jwe->cek
        _cjose_release_cek(&jwe->cek, jwe->cek_len);
        if (!_cjose_jwe_malloc(keysize, false, &jwe->cek, err))
        {
            return false;
        }
        memcpy(jwe->cek, jwk->keydata, keysize);
        jwe->cek_len = keysize;
    }

    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_set_cek_aes_cbc(cjose_jwe_t *jwe, const cjose_jwk_t *jwk, bool random, cjose_err *err)
{

    if (NULL != jwe->cek)
    {
        return true;
    }

    // make sure we have an enc header
    json_t *enc_obj = json_object_get(jwe->hdr, CJOSE_HDR_ENC);
    if (NULL == enc_obj)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }
    const char *enc = json_string_value(enc_obj);

    // determine the CEK key size based on the encryption algorithm
    size_t keysize = 0;
    if (strcmp(enc, CJOSE_HDR_ENC_A128CBC_HS256) == 0)
        keysize = 32;
    if (strcmp(enc, CJOSE_HDR_ENC_A192CBC_HS384) == 0)
        keysize = 48;
    if (strcmp(enc, CJOSE_HDR_ENC_A256CBC_HS512) == 0)
        keysize = 64;

    if (NULL == jwk)
    {
        // allocate memory for the CEK and fill with random bytes or 0's
        _cjose_release_cek(&jwe->cek, jwe->cek_len);
        if (!_cjose_jwe_malloc(keysize, !random, &jwe->cek, err))
        {
            return false;
        }
        jwe->cek_len = keysize;
    }
    else
    {
        // if a JWK is provided, it must be a symmetric key of correct size
        if (CJOSE_JWK_KTY_OCT != cjose_jwk_get_kty(jwk, err) || jwk->keysize != keysize * 8 || NULL == jwk->keydata)
        {
            CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
            return false;
        }

        // copy the key material directly from jwk to the jwe->cek
        _cjose_release_cek(&jwe->cek, jwe->cek_len);
        if (!_cjose_jwe_malloc(keysize, false, &jwe->cek, err))
        {
            return false;
        }
        memcpy(jwe->cek, jwk->keydata, keysize);
        jwe->cek_len = keysize;
    }
    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool
_cjose_jwe_encrypt_ek_dir(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err)
{
    // for direct encryption, JWE sec 5.1, step 6: let CEK be the symmetric key.
    if (!jwe->fns.set_cek(jwe, jwk, false, err))
    {
        return false;
    }

    // for direct encryption, JWE sec 5.1, step 5: let EK be empty octet seq.
    recipient->enc_key.raw = NULL;
    recipient->enc_key.raw_len = 0;

    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool
_cjose_jwe_decrypt_ek_dir(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err)
{
    // do not try and decrypt the ek. that's impossible.
    // instead... only try to realize the truth.  there is no ek.
    return jwe->fns.set_cek(jwe, jwk, false, err);
}

////////////////////////////////////////////////////////////////////////////////
static bool
_cjose_jwe_encrypt_ek_aes_kw(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err)
{
    if (NULL == jwe || NULL == jwk)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // jwk must be OCT
    if (jwk->kty != CJOSE_JWK_KTY_OCT)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // generate random CEK
    if (!jwe->fns.set_cek(jwe, NULL, true, err))
    {
        return false;
    }

    // create the AES encryption key from the shared key
    AES_KEY akey;
    if (AES_set_encrypt_key(jwk->keydata, jwk->keysize, &akey) < 0)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        return false;
    }

    // allocate buffer for encrypted CEK (=cek_len + 8)
    if (!_cjose_jwe_malloc(jwe->cek_len + 8, false, &recipient->enc_key.raw, err))
    {
        return false;
    }

    // AES wrap the CEK
    int len = AES_wrap_key(&akey, NULL, recipient->enc_key.raw, jwe->cek, jwe->cek_len);
    if (len <= 0)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        return false;
    }
    recipient->enc_key.raw_len = len;

    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool
_cjose_jwe_decrypt_ek_aes_kw(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err)
{
    if (NULL == jwe || NULL == jwk)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // jwk must be OCT
    if (jwk->kty != CJOSE_JWK_KTY_OCT)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // create the AES decryption key from the shared key
    AES_KEY akey;
    if (AES_set_decrypt_key(jwk->keydata, jwk->keysize, &akey) < 0)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        return false;
    }

    if (!jwe->fns.set_cek(jwe, NULL, false, err))
    {
        return false;
    }

    // AES unwrap the CEK in to jwe->cek
    int len = AES_unwrap_key(&akey, (const unsigned char *)NULL, jwe->cek, (const unsigned char *)recipient->enc_key.raw,
                             recipient->enc_key.raw_len);
    if (len <= 0)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        return false;
    }
    jwe->cek_len = len;

    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_encrypt_ek_rsa_padding(
    _jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, int padding, cjose_err *err)
{
    // jwk must be RSA
    if (jwk->kty != CJOSE_JWK_KTY_RSA || NULL == jwk->keydata)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // jwk must have the necessary public parts set
    BIGNUM *rsa_n = NULL, *rsa_e = NULL, *rsa_d = NULL;
    _cjose_jwk_rsa_get((RSA *)jwk->keydata, &rsa_n, &rsa_e, &rsa_d);
    if (NULL == rsa_e || NULL == rsa_n)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // generate random cek
    if (!jwe->fns.set_cek(jwe, NULL, true, err))
    {
        return false;
    }

    // the size of the ek will match the size of the RSA key
    recipient->enc_key.raw_len = RSA_size((RSA *)jwk->keydata);

    // for OAEP padding - the RSA size - 41 must be greater than input
    if (jwe->cek_len >= recipient->enc_key.raw_len - 41)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // allocate memory for RSA encryption
    cjose_get_dealloc()(recipient->enc_key.raw);
    if (!_cjose_jwe_malloc(recipient->enc_key.raw_len, false, &recipient->enc_key.raw, err))
    {
        return false;
    }

    // encrypt the CEK using RSA v1.5 or OAEP padding
    if (RSA_public_encrypt(jwe->cek_len, jwe->cek, recipient->enc_key.raw, (RSA *)jwk->keydata, padding)
        != recipient->enc_key.raw_len)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        return false;
    }

    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_decrypt_ek_rsa_padding(
    _jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, int padding, cjose_err *err)
{
    if (NULL == jwe || NULL == jwk)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // jwk must be RSA
    if (jwk->kty != CJOSE_JWK_KTY_RSA)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // we don't know the size of the key to expect, but must be < RSA_size
    _cjose_release_cek(&jwe->cek, jwe->cek_len);
    size_t buflen = RSA_size((RSA *)jwk->keydata);
    if (!_cjose_jwe_malloc(buflen, false, &jwe->cek, err))
    {
        return false;
    }

    // decrypt the CEK using RSA v1.5 or OAEP padding
    jwe->cek_len = RSA_private_decrypt(recipient->enc_key.raw_len, recipient->enc_key.raw, jwe->cek, (RSA *)jwk->keydata, padding);
    if (-1 == jwe->cek_len)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        return false;
    }

    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool
_cjose_jwe_encrypt_ek_rsa_oaep(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err)
{
    return _cjose_jwe_encrypt_ek_rsa_padding(recipient, jwe, jwk, RSA_PKCS1_OAEP_PADDING, err);
}

////////////////////////////////////////////////////////////////////////////////
static bool
_cjose_jwe_decrypt_ek_rsa_oaep(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err)
{
    return _cjose_jwe_decrypt_ek_rsa_padding(recipient, jwe, jwk, RSA_PKCS1_OAEP_PADDING, err);
}

////////////////////////////////////////////////////////////////////////////////
static bool
_cjose_jwe_encrypt_ek_rsa1_5(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err)
{
    return _cjose_jwe_encrypt_ek_rsa_padding(recipient, jwe, jwk, RSA_PKCS1_PADDING, err);
}

////////////////////////////////////////////////////////////////////////////////
static bool
_cjose_jwe_decrypt_ek_rsa1_5(_jwe_int_recipient_t *recipient, cjose_jwe_t *jwe, const cjose_jwk_t *jwk, cjose_err *err)
{
    return _cjose_jwe_decrypt_ek_rsa_padding(recipient, jwe, jwk, RSA_PKCS1_PADDING, err);
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_encrypt_ek_ecdh_es(_jwe_int_recipient_t *recipient,
                                          cjose_jwe_t *jwe,
                                          const cjose_jwk_t *jwk,
                                          cjose_err *err)
{
    cjose_jwk_t *epk_jwk = NULL;
    char *epk_json = NULL;
    uint8_t *secret = NULL;
    size_t secret_len = 0;
    uint8_t *otherinfo = NULL;
    size_t otherinfo_len = 0;
    uint8_t *derived = NULL;
    bool result = false;

    // generate and export random EPK
    epk_jwk = cjose_jwk_create_EC_random(cjose_jwk_EC_get_curve(jwk, err), err);
    if (NULL == epk_jwk)
    {
        // error details already set
        goto cjose_encrypt_ek_ecdh_es_finish;
    }
    epk_json = cjose_jwk_to_json(epk_jwk, false, err);
    if (NULL == epk_json)
    {
        goto cjose_encrypt_ek_ecdh_es_finish;
    }
    if (!cjose_header_set_raw(jwe->hdr, CJOSE_HDR_EPK, epk_json, err))
    {
        goto cjose_encrypt_ek_ecdh_es_finish;
    }

    // perform ECDH (private=epk_jwk, public=jwk)
    if (!cjose_jwk_derive_ecdh_bits(epk_jwk, jwk, &secret, &secret_len, err))
    {
        goto cjose_encrypt_ek_ecdh_es_finish;
    }

    // perform label, ConcatKDF
    // - assemble otherInfo from:
    //   * alg (== {enc})
    //   * apu (default = "")
    //   * apv (default = "")
    //   * keylen (determined from {enc})
    cjose_header_t *hdr = jwe->hdr;
    const char *algId = cjose_header_get(hdr, CJOSE_HDR_ENC, err);
    const size_t keylen = _keylen_from_enc(algId) / 8;

    if (!cjose_concatkdf_create_otherinfo(algId, keylen * 8, hdr, &otherinfo, &otherinfo_len, err))
    {
        goto cjose_encrypt_ek_ecdh_es_finish;
    }

    derived = cjose_concatkdf_derive(keylen, secret, secret_len, otherinfo, otherinfo_len, err);
    if (NULL == derived)
    {
        goto cjose_encrypt_ek_ecdh_es_finish;
    }

    jwe->cek = derived;
    jwe->cek_len = keylen;
    recipient->enc_key.raw = NULL;
    recipient->enc_key.raw_len = 0;
    result = true;

cjose_encrypt_ek_ecdh_es_finish:

    cjose_jwk_release(epk_jwk);
    cjose_get_dealloc()(epk_json);
    cjose_get_dealloc()(secret);
    cjose_get_dealloc()(otherinfo);

    return result;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_decrypt_ek_ecdh_es(_jwe_int_recipient_t *recipient,
                                          cjose_jwe_t *jwe,
                                          const cjose_jwk_t *jwk,
                                          cjose_err *err)
{
    cjose_jwk_t *epk_jwk = NULL;
    uint8_t *secret = NULL;
    size_t secret_len = 0;
    uint8_t *otherinfo = NULL;
    size_t otherinfo_len = 0;
    uint8_t *derived = NULL;
    bool result = false;

    memset(err, 0, sizeof(cjose_err));
    char *epk_json = cjose_header_get_raw(jwe->hdr, CJOSE_HDR_EPK, err);
    if (NULL != epk_json)
    {
        epk_jwk = cjose_jwk_import(epk_json, strlen(epk_json), err);
    }
    else if (CJOSE_ERR_NONE == err->code)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        goto cjose_decrypt_ek_ecdh_es_finish;
    }

    if (NULL == epk_jwk)
    {
        // error details already set
        goto cjose_decrypt_ek_ecdh_es_finish;
    }

    // perform ECDH (private=jwk, public=epk_jwk)
    if (!cjose_jwk_derive_ecdh_bits(jwk, epk_jwk, &secret, &secret_len, err))
    {
        goto cjose_decrypt_ek_ecdh_es_finish;
    }

    // perform label, ConcatKDF
    // - assemble otherInfo from:
    //   * alg (== {enc})
    //   * apu (default = "")
    //   * apv (default = "")
    //   * keylen (determined from {enc})
    cjose_header_t *hdr = jwe->hdr;
    const char *algId = cjose_header_get(hdr, CJOSE_HDR_ENC, err);
    const size_t keylen = _keylen_from_enc(algId) / 8;

    if (!cjose_concatkdf_create_otherinfo(algId, keylen * 8, hdr, &otherinfo, &otherinfo_len, err))
    {
        goto cjose_decrypt_ek_ecdh_es_finish;
    }

    derived = cjose_concatkdf_derive(keylen, secret, secret_len, otherinfo, otherinfo_len, err);
    if (NULL == derived)
    {
        goto cjose_decrypt_ek_ecdh_es_finish;
    }

    jwe->cek = derived;
    jwe->cek_len = keylen;
    recipient->enc_key.raw = NULL;
    recipient->enc_key.raw_len = 0;
    result = true;

cjose_decrypt_ek_ecdh_es_finish:

    cjose_jwk_release(epk_jwk);
    cjose_get_dealloc()(epk_json);
    cjose_get_dealloc()(secret);
    cjose_get_dealloc()(otherinfo);

    return result;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_set_iv_a256gcm(cjose_jwe_t *jwe, cjose_err *err)
{
    // generate IV as random 96 bit value
    cjose_get_dealloc()(jwe->enc_iv.raw);
    jwe->enc_iv.raw_len = 12;
    if (!_cjose_jwe_malloc(jwe->enc_iv.raw_len, true, &jwe->enc_iv.raw, err))
    {
        return false;
    }

    return true;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_set_iv_aes_cbc(cjose_jwe_t *jwe, cjose_err *err)
{
    // make sure we have an enc header
    json_t *enc_obj = json_object_get(jwe->hdr, CJOSE_HDR_ENC);
    if (NULL == enc_obj)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }
    const char *enc = json_string_value(enc_obj);

    cjose_get_dealloc()(jwe->enc_iv.raw);
    jwe->enc_iv.raw_len = 0;

    if (strcmp(enc, CJOSE_HDR_ENC_A128CBC_HS256) == 0)
        jwe->enc_iv.raw_len = 16;
    if (strcmp(enc, CJOSE_HDR_ENC_A192CBC_HS384) == 0)
        jwe->enc_iv.raw_len = 24;
    if (strcmp(enc, CJOSE_HDR_ENC_A256CBC_HS512) == 0)
        jwe->enc_iv.raw_len = 32;

    if (jwe->enc_iv.raw_len == 0)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // generate IV as random iv_size * 8 bit value
    if (!_cjose_jwe_malloc(jwe->enc_iv.raw_len, true, &jwe->enc_iv.raw, err))
    {
        return false;
    }

    return true;
}

#if defined(CJOSE_OPENSSL_11X)
#define CJOSE_EVP_CTRL_GCM_GET_TAG EVP_CTRL_AEAD_GET_TAG
#define CJOSE_EVP_CTRL_GCM_SET_TAG EVP_CTRL_AEAD_SET_TAG
#else
#define CJOSE_EVP_CTRL_GCM_GET_TAG EVP_CTRL_GCM_GET_TAG
#define CJOSE_EVP_CTRL_GCM_SET_TAG EVP_CTRL_GCM_SET_TAG
#endif

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_encrypt_dat_a256gcm(cjose_jwe_t *jwe, const uint8_t *plaintext, size_t plaintext_len, cjose_err *err)
{
    EVP_CIPHER_CTX *ctx = NULL;

    if (NULL == plaintext)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        goto _cjose_jwe_encrypt_dat_fail;
    }

    // get A256GCM cipher
    const EVP_CIPHER *cipher = EVP_aes_256_gcm();
    if (NULL == cipher)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_fail;
    }

    // instantiate and initialize a new openssl cipher context
    ctx = EVP_CIPHER_CTX_new();
    if (NULL == ctx)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_fail;
    }
    EVP_CIPHER_CTX_init(ctx);

    // initialize context for encryption using A256GCM cipher and CEK and IV
    if (EVP_EncryptInit_ex(ctx, cipher, NULL, jwe->cek, jwe->enc_iv.raw) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_fail;
    }

    // we need the header in base64url encoding as input for encryption
    if ((NULL == jwe->enc_header.b64u) && (!cjose_base64url_encode((const uint8_t *)jwe->enc_header.raw, jwe->enc_header.raw_len,
                                                                   &jwe->enc_header.b64u, &jwe->enc_header.b64u_len, err)))
    {
        goto _cjose_jwe_encrypt_dat_fail;
    }

    // set GCM mode AAD data (hdr_b64u) by setting "out" to NULL
    int bytes_encrypted = 0;
    if (EVP_EncryptUpdate(ctx, NULL, &bytes_encrypted, (unsigned char *)jwe->enc_header.b64u, jwe->enc_header.b64u_len) != 1
        || bytes_encrypted != jwe->enc_header.b64u_len)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_fail;
    }

    // allocate buffer for the ciphertext
    cjose_get_dealloc()(jwe->enc_ct.raw);
    jwe->enc_ct.raw_len = plaintext_len;
    if (!_cjose_jwe_malloc(jwe->enc_ct.raw_len, false, &jwe->enc_ct.raw, err))
    {
        goto _cjose_jwe_encrypt_dat_fail;
    }

    // encrypt entire plaintext to ciphertext buffer
    if (EVP_EncryptUpdate(ctx, jwe->enc_ct.raw, &bytes_encrypted, plaintext, plaintext_len) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_fail;
    }
    jwe->enc_ct.raw_len = bytes_encrypted;

    // finalize the encryption and set the ciphertext length to correct value
    if (EVP_EncryptFinal_ex(ctx, NULL, &bytes_encrypted) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_fail;
    }

    // allocate buffer for the authentication tag
    cjose_get_dealloc()(jwe->enc_auth_tag.raw);
    jwe->enc_auth_tag.raw_len = 16;
    if (!_cjose_jwe_malloc(jwe->enc_auth_tag.raw_len, false, &jwe->enc_auth_tag.raw, err))
    {
        goto _cjose_jwe_encrypt_dat_fail;
    }

    // get the GCM-mode authentication tag
    if (EVP_CIPHER_CTX_ctrl(ctx, CJOSE_EVP_CTRL_GCM_GET_TAG, jwe->enc_auth_tag.raw_len, jwe->enc_auth_tag.raw) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_fail;
    }

    EVP_CIPHER_CTX_free(ctx);
    return true;

_cjose_jwe_encrypt_dat_fail:
    if (NULL != ctx)
    {
        EVP_CIPHER_CTX_free(ctx);
    }
    return false;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_calc_auth_tag(const char *enc, cjose_jwe_t *jwe, uint8_t *md, unsigned int *md_len, cjose_err *err)
{
    bool retval = false;
    const EVP_MD *hash = NULL;

    if (strcmp(enc, CJOSE_HDR_ENC_A128CBC_HS256) == 0)
    {
        hash = EVP_sha256();
    }
    else if (strcmp(enc, CJOSE_HDR_ENC_A192CBC_HS384) == 0)
    {
        hash = EVP_sha384();
    }
    else if (strcmp(enc, CJOSE_HDR_ENC_A256CBC_HS512) == 0)
    {
        hash = EVP_sha512();
    }

    if (NULL == hash)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    uint8_t *msg = NULL;

    // calculate the Authentication Tag value over AAD + IV + ciphertext + AAD length

    // 0 = header
    // 1 = cek
    // 2 = iv
    // 3 = ciphertext
    // 4 = authentication tag

    // Additional Authentication Data length (base64encoded header) in # of bits in 64 bit length field
    uint64_t al = jwe->enc_header.b64u_len * 8;

    // concatenate AAD + IV + ciphertext + AAD length field
    int msg_len = jwe->enc_header.b64u_len + jwe->enc_iv.raw_len + jwe->enc_ct.raw_len + sizeof(uint64_t);
    if (!_cjose_jwe_malloc(msg_len, false, &msg, err))
    {
        goto _cjose_jwe_calc_auth_tag_end;
    }

    // construct AAD + IV + ciphertext + AAD input
    uint8_t *p = msg;
    memcpy(p, jwe->enc_header.b64u, jwe->enc_header.b64u_len);
    p += jwe->enc_header.b64u_len;
    memcpy(p, jwe->enc_iv.raw, jwe->enc_iv.raw_len);
    p += jwe->enc_iv.raw_len;
    memcpy(p, jwe->enc_ct.raw, jwe->enc_ct.raw_len);
    p += jwe->enc_ct.raw_len;

    // check if we are on a big endian or little endian machine
    int c = 1;
    if (*(char *)&c == 1)
    {
        // little endian machine: reverse AAD length for big endian representation
        al = (al & 0x00000000FFFFFFFF) << 32 | (al & 0xFFFFFFFF00000000) >> 32;
        al = (al & 0x0000FFFF0000FFFF) << 16 | (al & 0xFFFF0000FFFF0000) >> 16;
        al = (al & 0x00FF00FF00FF00FF) << 8 | (al & 0xFF00FF00FF00FF00) >> 8;
    }
    memcpy(p, &al, sizeof(uint64_t));

    // HMAC the input
    if (!HMAC(hash, jwe->cek, jwe->cek_len / 2, msg, msg_len, md, md_len))
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_calc_auth_tag_end;
    }

    // use only the first half of the bits
    *md_len = *md_len / 2;
    retval = true;

_cjose_jwe_calc_auth_tag_end:
    if (msg)
    {
        cjose_get_dealloc()(msg);
    }
    return retval;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_encrypt_dat_aes_cbc(cjose_jwe_t *jwe, const uint8_t *plaintext, size_t plaintext_len, cjose_err *err)
{
    // make sure we have an enc header
    json_t *enc_obj = json_object_get(jwe->hdr, CJOSE_HDR_ENC);
    if (NULL == enc_obj)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }
    const char *enc = json_string_value(enc_obj);

    // get the AES cipher
    EVP_CIPHER_CTX *ctx = NULL;
    const EVP_CIPHER *cipher = NULL;

    if (strcmp(enc, CJOSE_HDR_ENC_A128CBC_HS256) == 0)
        cipher = EVP_aes_128_cbc();
    if (strcmp(enc, CJOSE_HDR_ENC_A192CBC_HS384) == 0)
        cipher = EVP_aes_192_cbc();
    if (strcmp(enc, CJOSE_HDR_ENC_A256CBC_HS512) == 0)
        cipher = EVP_aes_256_cbc();

    if (NULL == cipher)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_aes_cbc_fail;
    }

    // instantiate and initialize a new openssl cipher context
    ctx = EVP_CIPHER_CTX_new();
    if (NULL == ctx)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_aes_cbc_fail;
    }
    EVP_CIPHER_CTX_init(ctx);

    // initialize context for decryption using the cipher, the 2nd half of the CEK and the IV
    if (EVP_EncryptInit_ex(ctx, cipher, NULL, jwe->cek + jwe->cek_len / 2, jwe->enc_iv.raw) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_aes_cbc_fail;
    }

    // we need the header in base64url encoding as input for encryption
    if ((NULL == jwe->enc_header.b64u) && (!cjose_base64url_encode((const uint8_t *)jwe->enc_header.raw, jwe->enc_header.raw_len,
                                                                   &jwe->enc_header.b64u, &jwe->enc_header.b64u_len, err)))
    {
        goto _cjose_jwe_encrypt_dat_aes_cbc_fail;
    }
    // allocate buffer for the ciphertext (plaintext + block size)
    cjose_get_dealloc()(jwe->enc_ct.raw);
    jwe->enc_ct.raw_len = plaintext_len + EVP_CIPHER_block_size(cipher);
    if (!_cjose_jwe_malloc(jwe->enc_ct.raw_len, false, &jwe->enc_ct.raw, err))
    {
        goto _cjose_jwe_encrypt_dat_aes_cbc_fail;
    }

    // encrypt entire plaintext to ciphertext buffer
    int bytes_encrypted = 0;
    if (EVP_EncryptUpdate(ctx, jwe->enc_ct.raw, &bytes_encrypted, plaintext, plaintext_len) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_aes_cbc_fail;
    }
    jwe->enc_ct.raw_len = bytes_encrypted;

    // finalize the encryption and set the ciphertext length to correct value
    if (EVP_EncryptFinal_ex(ctx, jwe->enc_ct.raw + bytes_encrypted, &bytes_encrypted) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_encrypt_dat_aes_cbc_fail;
    }
    jwe->enc_ct.raw_len += bytes_encrypted;

    // calculate Authentication Tag
    unsigned int tag_len = 0;
    uint8_t tag[EVP_MAX_MD_SIZE];
    if (_cjose_jwe_calc_auth_tag(enc, jwe, (unsigned char *)&tag, &tag_len, err) == false)
    {
        return false;
    }

    // allocate buffer for the authentication tag
    cjose_get_dealloc()(jwe->enc_auth_tag.raw);
    jwe->enc_auth_tag.raw_len = tag_len;
    if (!_cjose_jwe_malloc(jwe->enc_auth_tag.raw_len, false, &jwe->enc_auth_tag.raw, err))
    {
        goto _cjose_jwe_encrypt_dat_aes_cbc_fail;
    }

    memcpy(jwe->enc_auth_tag.raw, tag, tag_len);

    EVP_CIPHER_CTX_free(ctx);

    return true;

_cjose_jwe_encrypt_dat_aes_cbc_fail:
    if (NULL != ctx)
    {
        EVP_CIPHER_CTX_free(ctx);
    }
    return false;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_decrypt_dat_a256gcm(cjose_jwe_t *jwe, cjose_err *err)
{
    EVP_CIPHER_CTX *ctx = NULL;

    // get A256GCM cipher
    const EVP_CIPHER *cipher = EVP_aes_256_gcm();
    if (NULL == cipher)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_a256gcm_fail;
    }

    // instantiate and initialize a new openssl cipher context
    ctx = EVP_CIPHER_CTX_new();
    if (NULL == ctx)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_a256gcm_fail;
    }
    EVP_CIPHER_CTX_init(ctx);

    // initialize context for decryption using A256GCM cipher and CEK and IV
    if (EVP_DecryptInit_ex(ctx, cipher, NULL, jwe->cek, jwe->enc_iv.raw) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_a256gcm_fail;
    }

    // set the expected GCM-mode authentication tag
    if (EVP_CIPHER_CTX_ctrl(ctx, CJOSE_EVP_CTRL_GCM_SET_TAG, jwe->enc_auth_tag.raw_len, jwe->enc_auth_tag.raw) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_a256gcm_fail;
    }

    // set GCM mode AAD data (hdr_b64u) by setting "out" to NULL
    int bytes_decrypted = 0;
    if (EVP_DecryptUpdate(ctx, NULL, &bytes_decrypted, (unsigned char *)jwe->enc_header.b64u, jwe->enc_header.b64u_len) != 1
        || bytes_decrypted != jwe->enc_header.b64u_len)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_a256gcm_fail;
    }

    // allocate buffer for the plaintext
    cjose_get_dealloc()(jwe->dat);
    jwe->dat_len = jwe->enc_ct.raw_len;
    if (!_cjose_jwe_malloc(jwe->dat_len, false, &jwe->dat, err))
    {
        goto _cjose_jwe_decrypt_dat_a256gcm_fail;
    }

    // decrypt ciphertext to plaintext buffer
    if (EVP_DecryptUpdate(ctx, jwe->dat, &bytes_decrypted, jwe->enc_ct.raw, jwe->enc_ct.raw_len) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_a256gcm_fail;
    }
    jwe->dat_len = bytes_decrypted;

    // finalize the decryption
    if (EVP_DecryptFinal_ex(ctx, NULL, &bytes_decrypted) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_a256gcm_fail;
    }

    EVP_CIPHER_CTX_free(ctx);
    return true;

_cjose_jwe_decrypt_dat_a256gcm_fail:
    if (NULL != ctx)
    {
        EVP_CIPHER_CTX_free(ctx);
    }
    return false;
}

////////////////////////////////////////////////////////////////////////////////
static bool _cjose_jwe_decrypt_dat_aes_cbc(cjose_jwe_t *jwe, cjose_err *err)
{
    // make sure we have an enc header
    json_t *enc_obj = json_object_get(jwe->hdr, CJOSE_HDR_ENC);
    if (NULL == enc_obj)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }
    const char *enc = json_string_value(enc_obj);

    // calculate Authentication Tag
    unsigned int tag_len = 0;
    uint8_t tag[EVP_MAX_MD_SIZE];
    if (_cjose_jwe_calc_auth_tag(enc, jwe, (unsigned char *)&tag, &tag_len, err) == false)
    {
        return false;
    }

    // compare the provided Authentication Tag against our calculation
    if ((tag_len != jwe->enc_auth_tag.raw_len) || (cjose_const_memcmp(tag, jwe->enc_auth_tag.raw, tag_len) != 0))
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        return false;
    }

    // get the AES cipher
    EVP_CIPHER_CTX *ctx = NULL;
    const EVP_CIPHER *cipher = NULL;

    if (strcmp(enc, CJOSE_HDR_ENC_A128CBC_HS256) == 0)
    {
        cipher = EVP_aes_128_cbc();
    }
    else if (strcmp(enc, CJOSE_HDR_ENC_A192CBC_HS384) == 0)
    {
        cipher = EVP_aes_192_cbc();
    }
    else if (strcmp(enc, CJOSE_HDR_ENC_A256CBC_HS512) == 0)
    {
        cipher = EVP_aes_256_cbc();
    }

    if (NULL == cipher)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_aes_cbc_fail;
    }

    // instantiate and initialize a new openssl cipher context
    ctx = EVP_CIPHER_CTX_new();
    if (NULL == ctx)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_aes_cbc_fail;
    }
    EVP_CIPHER_CTX_init(ctx);

    // initialize context for decryption using the cipher, the 2nd half of the CEK and the IV
    if (EVP_DecryptInit_ex(ctx, cipher, NULL, jwe->cek + jwe->cek_len / 2, jwe->enc_iv.raw) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_aes_cbc_fail;
    }

    // allocate buffer for the plaintext + one block padding
    int p_len = jwe->enc_ct.raw_len, f_len = 0;
    cjose_get_dealloc()(jwe->dat);
    jwe->dat_len = p_len + AES_BLOCK_SIZE;
    if (!_cjose_jwe_malloc(jwe->dat_len, false, &jwe->dat, err))
    {
        goto _cjose_jwe_decrypt_dat_aes_cbc_fail;
    }

    // decrypt ciphertext to plaintext buffer
    if (EVP_DecryptUpdate(ctx, jwe->dat, &p_len, jwe->enc_ct.raw, jwe->enc_ct.raw_len) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_aes_cbc_fail;
    }

    // finalize the decryption
    if (EVP_DecryptFinal_ex(ctx, jwe->dat + p_len, &f_len) != 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
        goto _cjose_jwe_decrypt_dat_aes_cbc_fail;
    }
    jwe->dat_len = p_len + f_len;

    EVP_CIPHER_CTX_free(ctx);

    return true;

_cjose_jwe_decrypt_dat_aes_cbc_fail:
    if (NULL != ctx)
    {
        EVP_CIPHER_CTX_free(ctx);
    }
    return false;
}

////////////////////////////////////////////////////////////////////////////////
cjose_jwe_t *cjose_jwe_encrypt(
    const cjose_jwk_t *jwk, cjose_header_t *protected_header, const uint8_t *plaintext, size_t plaintext_len, cjose_err *err)
{

    cjose_jwe_recipient_t rec = {
        .jwk = jwk,
        .unprotected_header = NULL
    };

    return cjose_jwe_encrypt_multi(&rec, 1, protected_header, NULL, plaintext, plaintext_len, err);
}

////////////////////////////////////////////////////////////////////////////////
cjose_jwe_t *cjose_jwe_encrypt_multi(const cjose_jwe_recipient_t * recipients,
                                    size_t recipient_count,
                                    cjose_header_t *protected_header,
                                    cjose_header_t *shared_unprotected_header,
                                    const uint8_t *plaintext,
                                    size_t plaintext_len,
                                    cjose_err *err)
{
    cjose_jwe_t *jwe = NULL;

    if (NULL == recipients || NULL == protected_header || recipient_count < 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return NULL;
    }

    // allocate and initialize a new JWE object
    if (!_cjose_jwe_malloc(sizeof(cjose_jwe_t), false, (uint8_t **)&jwe, err))
    {
        return NULL;
    }

    jwe->to_count = recipient_count;
    if (!_cjose_jwe_malloc(sizeof(_jwe_int_recipient_t) * recipient_count, false, (uint8_t **)&jwe->to, err))
    {
        cjose_jwe_release(jwe);
        return NULL;
    }

    if (!_cjose_jwe_validate_enc(jwe, protected_header, err))
    {
        cjose_jwe_release(jwe);
        return NULL;
    }

    // validate JWE header
    for (size_t i = 0; i < recipient_count; i++)
    {

        if (NULL == recipients[i].jwk)
        {
            CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
            cjose_jwe_release(jwe);
            return NULL;
        }

        jwe->to[i].unprotected = json_incref(recipients[i].unprotected_header);

        // make sure we have an alg header
        if (!_cjose_jwe_validate_alg(protected_header, jwe->to[i].unprotected, recipient_count > 1, jwe->to + i, err))
        {
            cjose_jwe_release(jwe);
            return NULL;
        }
    }

    // prepare JWE headers
    jwe->hdr = json_deep_copy(protected_header);
    if (jwe->hdr == NULL)
    {
        cjose_jwe_release(jwe);
        return NULL;
    }
    jwe->shared_hdr = json_incref(shared_unprotected_header);

    for (size_t i = 0; i < recipient_count; i++)
    {

        // build JWE content-encryption key and encrypted key
        if (!jwe->to[i].fns.encrypt_ek(jwe->to + i, jwe, recipients[i].jwk, err))
        {
            cjose_jwe_release(jwe);
            return NULL;
        }
    }

    // build JWE header
    if (!_cjose_jwe_build_hdr(jwe, err))
    {
        cjose_jwe_release(jwe);
        return NULL;
    }

    // build JWE initialization vector
    if (!jwe->fns.set_iv(jwe, err))
    {
        cjose_jwe_release(jwe);
        return NULL;
    }

    // build JWE encrypted data and authentication tag
    if (!jwe->fns.encrypt_dat(jwe, plaintext, plaintext_len, err))
    {
        cjose_jwe_release(jwe);
        return NULL;
    }

    _cjose_release_cek(&jwe->cek, jwe->cek_len);

    return jwe;
}

////////////////////////////////////////////////////////////////////////////////
void cjose_jwe_release(cjose_jwe_t *jwe)
{
    if (NULL == jwe)
    {
        return;
    }

    json_decref(jwe->hdr);
    json_decref(jwe->shared_hdr);

    _cjose_dealloc_part(&jwe->enc_header);
    _cjose_dealloc_part(&jwe->enc_iv);
    _cjose_dealloc_part(&jwe->enc_ct);
    _cjose_dealloc_part(&jwe->enc_auth_tag);

    for (int i = 0; i < jwe->to_count; ++i)
    {
        json_decref(jwe->to[i].unprotected);
        _cjose_dealloc_part(&jwe->to[i].enc_key);
    }

    cjose_get_dealloc()(jwe->to);

    _cjose_release_cek(&jwe->cek, jwe->cek_len);

    cjose_get_dealloc()(jwe->dat);
    cjose_get_dealloc()(jwe);
}

////////////////////////////////////////////////////////////////////////////////
char *cjose_jwe_export(cjose_jwe_t *jwe, cjose_err *err)
{
    char *cser = NULL;
    size_t cser_len = 0;

    if (NULL == jwe || jwe->to_count > 1 || !_cjose_empty_json(jwe->shared_hdr) || !_cjose_empty_json(jwe->to[0].unprotected))
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return NULL;
    }

    if (!_cjose_convert_to_base64(jwe, err))
    {
        return NULL;
    }

    // make sure all parts are b64u encoded
    cser_len = jwe->enc_header.b64u_len + jwe->to[0].enc_key.b64u_len + jwe->enc_iv.b64u_len + jwe->enc_ct.b64u_len
               + jwe->enc_auth_tag.b64u_len + 5;

    // allocate buffer for compact serialization
    if (!_cjose_jwe_malloc(cser_len, false, (uint8_t **)&cser, err))
    {
        return NULL;
    }

    // build the compact serialization
    snprintf(cser, cser_len, "%s.%s.%s.%s.%s", jwe->enc_header.b64u, jwe->to[0].enc_key.b64u, jwe->enc_iv.b64u, jwe->enc_ct.b64u,
             jwe->enc_auth_tag.b64u);

    return cser;
}

////////////////////////////////////////////////////////////////////////////////
static inline bool _cjose_add_json_part(json_t *obj, const char *key, struct _cjose_jwe_part_int *part, cjose_err *err)
{
    json_t *str = json_stringn(part->b64u, part->b64u_len);
    if (NULL == str)
    {
        CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
        return false;
    }
    json_object_set_new(obj, key, str);
    return true;
}

////////////////////////////////////////////////////////////////////////////////
char *cjose_jwe_export_json(cjose_jwe_t *jwe, cjose_err *err)
{

    if (!_cjose_convert_to_base64(jwe, err))
    {
        return NULL;
    }

    json_t *form = json_object();
    if (NULL == form)
    {
        CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
        return NULL;
    }

    if (!_cjose_add_json_part(form, "protected", &jwe->enc_header, err) || !_cjose_add_json_part(form, "iv", &jwe->enc_iv, err)
        || !_cjose_add_json_part(form, "ciphertext", &jwe->enc_ct, err)
        || !_cjose_add_json_part(form, "tag", &jwe->enc_auth_tag, err))
    {
        json_delete(form);
        return NULL;
    }

    json_object_set(form, "unprotected", jwe->shared_hdr);

    if (jwe->to_count == 1)
    {
        json_object_set(form, "header", jwe->to[0].unprotected);
        if (!_cjose_add_json_part(form, "encrypted_key", &jwe->to[0].enc_key, err))
        {
            json_delete(form);
            return NULL;
        }
    }
    else
    {

        json_t *recipients = json_array();
        if (NULL == recipients)
        {
            CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
            json_delete(form);
            return NULL;
        }

        json_object_set_new(form, "recipients", recipients);

        for (int i = 0; i < jwe->to_count; i++)
        {

            json_t *recipient = json_object();
            if (NULL == recipient)
            {
                CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
                json_delete(form);
                return NULL;
            }

            json_array_append_new(recipients, recipient);

            json_object_set(recipient, "header", jwe->to[i].unprotected);
            if (!_cjose_add_json_part(recipient, "encrypted_key", &jwe->to[i].enc_key, err))
            {
                json_delete(form);
                return NULL;
            }
        }
    }

    char *json_str = json_dumps(form, 0);
    if (NULL == json_str)
    {
        CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
        json_delete(form);
        return NULL;
    }

    json_delete(form);
    return json_str;
}

////////////////////////////////////////////////////////////////////////////////
static bool
_cjose_jwe_import_part(struct _cjose_jwe_part_int *part, bool empty_ok, const char *b64u, size_t b64u_len, cjose_err *err)
{
    // only the ek and the data parts may be of zero length
    if (b64u_len == 0 && !empty_ok)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // copy the b64u part to the jwe
    part->b64u = _cjose_strndup(b64u, b64u_len, err);
    part->b64u_len = b64u_len;

    // b64u decode the part
    if (!cjose_base64url_decode(part->b64u, part->b64u_len, (uint8_t **)&part->raw, &part->raw_len, err) || NULL == part->raw)
    {
        return false;
    }

    return true;
}

static bool _cjose_jwe_import_json_part(struct _cjose_jwe_part_int *part, bool empty_ok, json_t *json, cjose_err *err)
{

    if (NULL == json || !json_is_string(json))
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    const char *str = json_string_value(json);
    // TODO: if json_is_string() was true, are we guaranteed that str is !NULL?

    return _cjose_jwe_import_part(part, empty_ok, str, strlen(str), err);
}

////////////////////////////////////////////////////////////////////////////////
cjose_jwe_t *cjose_jwe_import(const char *cser, size_t cser_len, cjose_err *err)
{
    cjose_jwe_t *jwe = NULL;

    if (NULL == cser)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return NULL;
    }

    // allocate and initialize a new JWE object
    if (!_cjose_jwe_malloc(sizeof(cjose_jwe_t), false, (uint8_t **)&jwe, err))
    {
        return NULL;
    }

    jwe->to_count = 1;
    if (!_cjose_jwe_malloc(sizeof(_jwe_int_recipient_t), false, (uint8_t **)&jwe->to, err))
    {
        cjose_jwe_release(jwe);
        return NULL;
    }

    struct _cjose_jwe_part_int *parts[] = {
        &jwe->enc_header, &jwe->to[0].enc_key, &jwe->enc_iv, &jwe->enc_ct, &jwe->enc_auth_tag,
    };

    // import each part of the compact serialization
    int part = 0;
    size_t idx = 0;
    size_t start_idx = 0;
    while (idx <= cser_len && part < 5)
    {
        if ((idx == cser_len) || (cser[idx] == '.'))
        {
            if (!_cjose_jwe_import_part(parts[part], 1 == part || 3 == part, cser + start_idx, idx - start_idx, err))
            {
                cjose_jwe_release(jwe);
                return NULL;
            }
            part++;
            start_idx = idx + 1;
        }
        if (part < 5)
            ++idx;
    }

    // fail if we didn't find enough parts
    if (part != 5)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        cjose_jwe_release(jwe);
        return NULL;
    }

    // fail if we finished early (e.g. more than 5 parts)
    if (idx != cser_len)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        cjose_jwe_release(jwe);
        return NULL;
    }

    // deserialize JSON header
    jwe->hdr = _cjose_parse_json_object((const char *)jwe->enc_header.raw, jwe->enc_header.raw_len, err);
    if (NULL == jwe->hdr)
    {
        cjose_jwe_release(jwe);
        return NULL;
    }

    // validate the JSON header. No unprotected headers can exist.
    if (!_cjose_jwe_validate_alg((cjose_header_t *)jwe->hdr, NULL, false, jwe->to, err)
        || !_cjose_jwe_validate_enc(jwe, (cjose_header_t *)jwe->hdr, err))
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        cjose_jwe_release(jwe);
        return NULL;
    }

    return jwe;
}

static inline bool _cjose_read_json_recipient(cjose_jwe_t *jwe,
                                              cjose_header_t *protected_header,
                                              bool is_multiple,
                                              _jwe_int_recipient_t *recipient,
                                              json_t *obj,
                                              cjose_err *err)
{

    if (!json_is_object(obj))
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    if (!_cjose_jwe_import_json_part(&recipient->enc_key, true, json_object_get(obj, "encrypted_key"), err))
    {
        return false;
    };

    recipient->unprotected = json_incref(json_object_get(obj, "header"));

    // it's OK to have empty/null unprotected header
    if (recipient->unprotected && !json_is_object(recipient->unprotected))
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    return _cjose_jwe_validate_alg(protected_header, jwe->shared_hdr, is_multiple, recipient, err);
}

////////////////////////////////////////////////////////////////////////////////
cjose_jwe_t *cjose_jwe_import_json(const char *cser, size_t cser_len, cjose_err *err)
{
    cjose_jwe_t *jwe = NULL;
    json_t *form = NULL;
    json_t *protected_header = NULL;

    if (NULL == cser)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return NULL;
    }

    // allocate and initialize a new JWE object
    if (!_cjose_jwe_malloc(sizeof(cjose_jwe_t), false, (uint8_t **)&jwe, err))
    {
        return NULL;
    }

    form = _cjose_parse_json_object(cser, cser_len, err);
    if (NULL == form)
    {
        goto _cjose_jwe_import_json_fail;
    }

    json_t *recipients = json_object_get(form, "recipients");
    if (NULL != recipients)
    {
        if (!json_is_array(recipients))
        {
            CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
            goto _cjose_jwe_import_json_fail;
        }
        jwe->to_count = json_array_size(recipients);
        if (jwe->to_count < 1)
        {
            // TODO: is empty recipients array allowed?
            CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
            goto _cjose_jwe_import_json_fail;
        }
    }
    else
    {
        jwe->to_count = 1;
    }

    if (!_cjose_jwe_malloc(sizeof(_jwe_int_recipient_t) * jwe->to_count, false, (uint8_t **)&jwe->to, err))
    {
        goto _cjose_jwe_import_json_fail;
    }

    if (!_cjose_jwe_import_json_part(&jwe->enc_header, false, json_object_get(form, "protected"), err))
    {
        goto _cjose_jwe_import_json_fail;
    }

    protected_header = _cjose_parse_json_object((const char *)jwe->enc_header.raw, jwe->enc_header.raw_len, err);
    if (NULL == protected_header)
    {
        goto _cjose_jwe_import_json_fail;
    }

    if (NULL == recipients)
    {

        if (!_cjose_read_json_recipient(jwe, protected_header, false, jwe->to, form, err))
        {
            goto _cjose_jwe_import_json_fail;
        }
    }
    else
    {

        for (size_t i = 0; i < jwe->to_count; i++)
        {

            if (!_cjose_read_json_recipient(jwe, protected_header, jwe->to_count > 1, jwe->to + i, json_array_get(recipients, i),
                                            err))
            {
                goto _cjose_jwe_import_json_fail;
            }
        }
    }

    if (!_cjose_jwe_validate_enc(jwe, protected_header, err))
    {
        goto _cjose_jwe_import_json_fail;
    }

    if (!_cjose_jwe_import_json_part(&jwe->enc_iv, false, json_object_get(form, "iv"), err)
        || !_cjose_jwe_import_json_part(&jwe->enc_ct, false, json_object_get(form, "ciphertext"), err)
        || !_cjose_jwe_import_json_part(&jwe->enc_auth_tag, false, json_object_get(form, "tag"), err))
    {

        goto _cjose_jwe_import_json_fail;
    }

    jwe->hdr = json_incref(protected_header);

    json_decref(form);
    json_decref(protected_header);

    return jwe;

_cjose_jwe_import_json_fail:
    json_decref(form);
    json_decref(protected_header);
    cjose_jwe_release(jwe);
    return NULL;
}

uint8_t *cjose_jwe_decrypt_multi(cjose_jwe_t *jwe, cjose_key_locator key_locator, void *data, size_t *content_len, cjose_err *err)
{

    uint8_t *cek = 0;
    size_t cek_len = 0;
    uint8_t *content = NULL;

    if (NULL == jwe || NULL == key_locator || NULL == content_len)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return NULL;
    }

    for (size_t i = 0; i < jwe->to_count; i++)
    {

        const cjose_jwk_t *key = key_locator(jwe, (cjose_header_t *)jwe->to[i].unprotected, data);
        if (NULL == key)
        {
            continue;
        }

        // decrypt JWE content-encryption key from encrypted key
        if (!jwe->to[i].fns.decrypt_ek(jwe->to + i, jwe, key, err))
        {
            // if one key failed to decrypt, fail everything.
            goto _cjose_jwe_decrypt_multi_fail;
        }

        if (NULL == cek)
        {
            cek_len = jwe->cek_len;
            cek = cjose_get_alloc()(cek_len);
            memcpy(cek, jwe->cek, cek_len);
        }
        else
        {
            if (cek_len != jwe->cek_len || memcmp(jwe->cek, cek, cek_len))
            {
                CJOSE_ERROR(err, CJOSE_ERR_CRYPTO);
                goto _cjose_jwe_decrypt_multi_fail;
            }
        }
    }

    // decrypt JWE encrypted data
    if (!jwe->fns.decrypt_dat(jwe, err))
    {
        goto _cjose_jwe_decrypt_multi_fail;
    }

    // take the plaintext data from the jwe object
    content = jwe->dat;
    *content_len = jwe->dat_len;

    jwe->dat = NULL;
    jwe->dat_len = 0;

_cjose_jwe_decrypt_multi_fail:

    _cjose_release_cek(&cek, cek_len);

    return content;
}

////////////////////////////////////////////////////////////////////////////////
uint8_t *cjose_jwe_decrypt(cjose_jwe_t *jwe, const cjose_jwk_t *jwk, size_t *content_len, cjose_err *err)
{
    if (NULL == jwe || NULL == jwk || NULL == content_len || jwe->to_count > 1)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return NULL;
    }

    // decrypt JWE content-encryption key from encrypted key
    if (!jwe->to[0].fns.decrypt_ek(jwe->to, jwe, jwk, err))
    {
        return NULL;
    }

    // decrypt JWE encrypted data
    if (!jwe->fns.decrypt_dat(jwe, err))
    {
        return NULL;
    }

    // take the plaintext data from the jwe object
    uint8_t *content = jwe->dat;
    *content_len = jwe->dat_len;
    jwe->dat = NULL;
    jwe->dat_len = 0;

    return content;
}

////////////////////////////////////////////////////////////////////////////////
cjose_header_t *cjose_jwe_get_protected(cjose_jwe_t *jwe)
{
    if (NULL == jwe)
    {
        return NULL;
    }
    return (cjose_header_t *)jwe->hdr;
}

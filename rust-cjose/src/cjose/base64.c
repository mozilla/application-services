/*
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */

#include <cjose/base64.h>
#include <cjose/util.h>

#include <errno.h>
#include <string.h>
#include <stdlib.h>
#include <assert.h>

// defines
#define B64_BYTE1(ptr) (((*ptr) & 0xfc) >> 2)
#define B64_BYTE2(ptr) ((((*ptr) & 0x03) << 4) | ((*(ptr + 1) & 0xf0) >> 4))
#define B64_BYTE3(ptr) (((*(ptr + 1) & 0x0f) << 2) | ((*(ptr + 2) & 0xc0) >> 6))
#define B64_BYTE4(ptr) (*(ptr + 2) & 0x3f)

// internal data

static const char *ALPHABET_B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
static const char *ALPHABET_B64U = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

static const uint8_t TEBAHPLA_B64[]
    = { 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0x3e, 0xff, 0x3e, 0xff, 0x3f, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0xff, 0xff, 0xff, 0xff, 0x3f, 0xff, 0x1a, 0x1b, 0x1c,
        0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30,
        0x31, 0x32, 0x33, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff };

// internal functions

static inline bool _decode(const char *input, size_t inlen, uint8_t **output, size_t *outlen, bool url, cjose_err *err)
{
    if ((NULL == input) || (NULL == output) || (NULL == outlen))
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // return empty string on 0 length input
    if (0 == inlen)
    {
        uint8_t *retVal = (uint8_t *)cjose_get_alloc()(sizeof(uint8_t));
        if (NULL == retVal)
        {
            CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
            return false;
        }

        retVal[0] = 0;
        *output = retVal;
        *outlen = 0;
        return true;
    }

    // extra validation -- inlen is a multiple of 4
    if ((!url && 0 != (inlen % 4)) || (inlen % 4 == 1))
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // rlen takes a best guess on size;
    // might be too large for base64url, but never too small.
    size_t rlen = ((inlen * 3) >> 2) + 3;
    uint8_t *buffer = cjose_get_alloc()(sizeof(uint8_t) * rlen);
    if (NULL == buffer)
    {
        CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
        return false;
    }

    size_t idx = 0;
    size_t pos = 0;
    size_t shift = 0;
    uint32_t packed = 0;
    while (inlen > idx)
    {
        uint8_t val;
        val = input[idx];
        if ('=' == val)
        {
            break;
        }
        else if (url && ('+' == val || '/' == val))
        {
            CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
            goto b64_decode_failed;
        }
        else if (!url && ('-' == val || '_' == val))
        {
            CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
            goto b64_decode_failed;
        }

        val = TEBAHPLA_B64[val];
        if (0xff == val)
        {
            CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
            cjose_get_dealloc()(buffer);
            return false;
        }
        idx++;

        packed = packed | (val << (18 - (6 * shift++)));
        if (4 == shift)
        {
            buffer[pos++] = (packed >> 16) & 0xff;
            buffer[pos++] = (packed >> 8) & 0xff;
            buffer[pos++] = packed & 0xff;
            shift = 0;
            packed = 0;
        }
    }

    assert(shift != 1);
    assert(shift != 4);

    if (shift == 3)
    {
        buffer[pos++] = (packed >> 16) & 0xff;
        buffer[pos++] = (packed >> 8) & 0xff;
    }

    if (shift == 2)
    {
        buffer[pos++] = (packed >> 16) & 0xff;
    }

    *output = buffer;
    *outlen = pos;
    assert(*outlen <= rlen);
    return true;

b64_decode_failed:
    if (NULL != buffer)
    {
        cjose_get_dealloc()(buffer);
    }
    return false;
}

static inline bool _encode(const uint8_t *input, size_t inlen, char **output, size_t *outlen, const char *alphabet, cjose_err *err)
{
    if ((inlen > 0 && NULL == input) || (NULL == output) || (NULL == outlen))
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    // return empty string on 0 length input
    if (!inlen)
    {
        char *retVal = (char *)cjose_get_alloc()(sizeof(char));
        if (!retVal)
        {
            CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
            return false;
        }
        retVal[0] = '\0';
        *output = retVal;
        *outlen = 0;
        return true;
    }

    const bool padit = (ALPHABET_B64 == alphabet);
    size_t rlen = (((inlen + 2) / 3) << 2);
    char *base;

    base = (char *)cjose_get_alloc()(sizeof(char) * (rlen + 1));
    if (NULL == base)
    {
        CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
        return false;
    }

    size_t pos = 0, idx = 0;
    while ((idx + 2) < inlen)
    {
        base[pos++] = alphabet[0x3f & (input[idx] >> 2)];
        base[pos++] = alphabet[(0x3f & (input[idx] << 4)) | (0x3f & (input[idx + 1] >> 4))];
        base[pos++] = alphabet[(0x3f & (input[idx + 1] << 2)) | (0x3f & (input[idx + 2] >> 6))];
        base[pos++] = alphabet[0x3f & input[idx + 2]];
        idx += 3;
    }

    if (idx < inlen)
    {
        if ((inlen - 1) == idx)
        {
            base[pos++] = alphabet[0x3f & (input[idx] >> 2)];
            base[pos++] = alphabet[0x3f & (input[idx] << 4)];
            if (padit)
            {
                base[pos++] = '=';
                base[pos++] = '=';
            }
        }
        else
        {
            base[pos++] = alphabet[0x3f & (input[idx] >> 2)];
            base[pos++] = alphabet[(0x3f & (input[idx] << 4)) | (0x3f & (input[idx + 1] >> 4))];
            base[pos++] = alphabet[0x3f & (input[idx + 1] << 2)];
            if (padit)
            {
                base[pos++] = '=';
            }
        }
        rlen = pos;
    }
    base[rlen] = '\0';

    *output = base;
    *outlen = rlen;
    return true;
}

// interface functions

bool cjose_base64_encode(const uint8_t *input, size_t inlen, char **output, size_t *outlen, cjose_err *err)
{
    return _encode(input, inlen, output, outlen, ALPHABET_B64, err);
}
bool cjose_base64url_encode(const uint8_t *input, size_t inlen, char **output, size_t *outlen, cjose_err *err)
{
    return _encode(input, inlen, output, outlen, ALPHABET_B64U, err);
}

bool cjose_base64_decode(const char *input, size_t inlen, uint8_t **output, size_t *outlen, cjose_err *err)
{
    return _decode(input, inlen, output, outlen, false, err);
}
bool cjose_base64url_decode(const char *input, size_t inlen, uint8_t **output, size_t *outlen, cjose_err *err)
{
    return _decode(input, inlen, output, outlen, true, err);
}

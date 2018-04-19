/*!
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */

#include <stdlib.h>
#include <jansson.h>
#include "cjose/header.h"
#include "include/header_int.h"

const char *CJOSE_HDR_ALG = "alg";
const char *CJOSE_HDR_ALG_NONE = "none";
const char *CJOSE_HDR_ALG_ECDH_ES = "ECDH-ES";
const char *CJOSE_HDR_ALG_RSA_OAEP = "RSA-OAEP";
const char *CJOSE_HDR_ALG_RSA1_5 = "RSA1_5";
const char *CJOSE_HDR_ALG_A128KW = "A128KW";
const char *CJOSE_HDR_ALG_A192KW = "A192KW";
const char *CJOSE_HDR_ALG_A256KW = "A256KW";
const char *CJOSE_HDR_ALG_DIR = "dir";
const char *CJOSE_HDR_ALG_PS256 = "PS256";
const char *CJOSE_HDR_ALG_PS384 = "PS384";
const char *CJOSE_HDR_ALG_PS512 = "PS512";
const char *CJOSE_HDR_ALG_RS256 = "RS256";
const char *CJOSE_HDR_ALG_RS384 = "RS384";
const char *CJOSE_HDR_ALG_RS512 = "RS512";
const char *CJOSE_HDR_ALG_HS256 = "HS256";
const char *CJOSE_HDR_ALG_HS384 = "HS384";
const char *CJOSE_HDR_ALG_HS512 = "HS512";
const char *CJOSE_HDR_ALG_ES256 = "ES256";
const char *CJOSE_HDR_ALG_ES384 = "ES384";
const char *CJOSE_HDR_ALG_ES512 = "ES512";

const char *CJOSE_HDR_ENC = "enc";
const char *CJOSE_HDR_ENC_A256GCM = "A256GCM";
const char *CJOSE_HDR_ENC_A128CBC_HS256 = "A128CBC-HS256";
const char *CJOSE_HDR_ENC_A192CBC_HS384 = "A192CBC-HS384";
const char *CJOSE_HDR_ENC_A256CBC_HS512 = "A256CBC-HS512";

const char *CJOSE_HDR_CTY = "cty";

const char *CJOSE_HDR_KID = "kid";

const char *CJOSE_HDR_EPK = "epk";

const char *CJOSE_HDR_APU = "apu";
const char *CJOSE_HDR_APV = "apv";

////////////////////////////////////////////////////////////////////////////////
cjose_header_t *cjose_header_new(cjose_err *err)
{
    cjose_header_t *retval = (cjose_header_t *)json_object();
    if (NULL == retval)
    {
        CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
    }
    return retval;
}

////////////////////////////////////////////////////////////////////////////////
cjose_header_t *cjose_header_retain(cjose_header_t *header)
{
    if (NULL != header)
    {
        header = (cjose_header_t *)json_incref((json_t *)header);
    }
    return header;
}

////////////////////////////////////////////////////////////////////////////////
void cjose_header_release(cjose_header_t *header)
{
    if (NULL != header)
    {
        json_decref((json_t *)header);
    }
}

////////////////////////////////////////////////////////////////////////////////
bool cjose_header_set(cjose_header_t *header, const char *attr, const char *value, cjose_err *err)
{
    if (NULL == header || NULL == attr || NULL == value)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    json_t *value_obj = json_string(value);
    if (NULL == value_obj)
    {
        CJOSE_ERROR(err, CJOSE_ERR_NO_MEMORY);
        return false;
    }

    json_object_set_new((json_t *)header, attr, value_obj);

    return true;
}

////////////////////////////////////////////////////////////////////////////////
const char *cjose_header_get(cjose_header_t *header, const char *attr, cjose_err *err)
{
    if (NULL == header || NULL == attr)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return NULL;
    }

    json_t *value_obj = json_object_get((json_t *)header, attr);
    if (NULL == value_obj)
    {
        return NULL;
    }

    return json_string_value(value_obj);
}

////////////////////////////////////////////////////////////////////////////////
bool cjose_header_set_raw(cjose_header_t *header, const char *attr, const char *value, cjose_err *err)
{
    if (NULL == header || NULL == attr || NULL == value)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    json_error_t j_err;
    json_t *value_obj = json_loads(value, 0, &j_err);
    if (NULL == value_obj)
    {
        // unfortunately, it's not possible to tell whether the error is due
        // to syntax, or memory shortage. See https://github.com/akheron/jansson/issues/352
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return false;
    }

    json_object_set_new((json_t *)header, attr, value_obj);

    return true;
}

////////////////////////////////////////////////////////////////////////////////
char *cjose_header_get_raw(cjose_header_t *header, const char *attr, cjose_err *err)
{
    if (NULL == header || NULL == attr)
    {
        CJOSE_ERROR(err, CJOSE_ERR_INVALID_ARG);
        return NULL;
    }

    json_t *value_obj = json_object_get((json_t *)header, attr);
    if (NULL == value_obj)
    {
        return NULL;
    }

    return json_dumps(value_obj, JSON_COMPACT);
}

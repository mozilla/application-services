/**
 *
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */

#include <openssl/err.h>
#include "cjose/error.h"

////////////////////////////////////////////////////////////////////////////////
static const char *_ERR_MSG_TABLE[] = { "no error", "invalid argument", "invalid state", "out of memory", "crypto error" };

////////////////////////////////////////////////////////////////////////////////
const char *cjose_err_message(cjose_errcode code)
{
    const char *retval = NULL;
    if (CJOSE_ERR_CRYPTO == code)
    {
        // for crypto errors, return the most recent openssl error as message
        long err = ERR_get_error();
        while (0 != err)
        {
            retval = ERR_error_string(err, NULL);
            err = ERR_get_error();
        }
    }
    if (NULL == retval)
    {
        retval = _ERR_MSG_TABLE[code];
    }
    return retval;
}

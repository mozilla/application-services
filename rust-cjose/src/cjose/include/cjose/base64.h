/*
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */
/**
 * \file
 * \brief
 * Functions for encoding to and decoding from base64 and base64url.
 *
 * \b NOTE: When successful, the output of each function MUST be
 * released by calling free(), even if the output is of 0 length.
 */

#ifndef CJOSE_BASE64_H
#define CJOSE_BASE64_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "cjose/error.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Encodes the given octet string to Base64.
 *
 * \param input The octet string to encode.
 * \param inlen The length of <tt>input</tt>.
 * \param output The encoded text string.
 * \param outlen The length of <tt>output</tt>
 *               (not including the terminating NULL).
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 */
bool cjose_base64_encode(const uint8_t *input, const size_t inlen, char **output, size_t *outlen, cjose_err *err);

/**
 * Encodes the given octet string to URL-safe Base64.
 *
 * \param input The octet string to encode.
 * \param inlen The length of <tt>input</tt>.
 * \param output The encoded output string.
 * \param outlen The length of <tt>output</tt>
 *               (not including the terminating NULL).
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 */
bool cjose_base64url_encode(const uint8_t *input, const size_t inlen, char **output, size_t *outlen, cjose_err *err);

/**
 * Decodes the given string from Base64.
 *
 * \b NOTE: <tt>output</tt> is \b NOT NULL-terminated.
 *
 * \param input The text string to decode.
 * \param inlen The length of <tt>input</tt>.
 * \param output The decoded octet string.
 * \param outlen The length of <tt>output</tt>.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 */
bool cjose_base64_decode(const char *input, const size_t inlen, uint8_t **output, size_t *outlen, cjose_err *err);

/**
 * Decodes the given string from URL-Safe Base64.
 *
 * \b NOTE: <tt>output</tt> is \b NOT NULL-terminated.
 *
 * \param input The text string to decode.
 * \param inlen The length of <tt>input</tt>.
 * \param output The decoded octet string.
 * \param outlen The length of <tt>output</tt>.
 * \param err [out] An optional error object which can be used to get additional
 *        information in the event of an error.
 */
bool cjose_base64url_decode(const char *input, const size_t inlen, uint8_t **output, size_t *outlen, cjose_err *err);

#ifdef __cplusplus
}
#endif

#endif // CJOSE_BASE64_H

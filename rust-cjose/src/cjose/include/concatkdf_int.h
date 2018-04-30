/*!
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2018 Cisco Systems, Inc.  All Rights Reserved.
 */

#ifndef SRC_CONCATKDF_INT_H
#define SRC_CONCATKDF_INT_H

#include <stddef.h>
#include <stdint.h>
#include <cjose/error.h>
#include <cjose/header.h>

bool cjose_concatkdf_create_otherinfo(const char *alg,
                                      size_t keylen,
                                      cjose_header_t *hdr,
                                      uint8_t **otherinfo, size_t *otherinfoLen,
                                      cjose_err *err);
uint8_t *cjose_concatkdf_derive(const size_t keylen,
                                const uint8_t *ikm, const size_t ikmLen,
                                const uint8_t *otherinfo, const size_t otherinfoLen,
                                cjose_err *err);

#endif // SRC_CONCATKDF_INT_H

/*
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */

/**
 * \file  util.h
 * \brief Utility functions and data structures for CJOSE.
 *
 */

#ifndef CJOSE_UTIL_H
#define CJOSE_UTIL_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

#include <openssl/rsa.h>

#ifdef __cplusplus
extern "C" {
#endif

#if OPENSSL_VERSION_NUMBER >= 0x10100005L && !defined(LIBRESSL_VERSION_NUMBER)
#define CJOSE_OPENSSL_11X
#endif

/**
 * Macro to explicitly mark a parameter unused, and usable across multiple
 * compiler/platform environments.
 */
#define CJOSE_UNUSED_PARAM(x) (void)(x)

/**
 * Typedef for the basic memory allocator function.
 */
typedef void *(*cjose_alloc_fn_t)(size_t);
/**
 * Typedef for the enhanced memory allocator function.
 */
typedef void *(*cjose_alloc3_fn_t)(size_t, const char *, int);

/**
 * Typedef for the basic memory reallocator function.
 */
typedef void *(*cjose_realloc_fn_t)(void *, size_t);
/**
 * Typedef for the enhanced memory reallocator function.
 */
typedef void *(*cjose_realloc3_fn_t)(void *, size_t, const char *, int);

/**
 * Typedef for the basic memory deallocator function.
 */
typedef void (*cjose_dealloc_fn_t)(void *);
/**
 * Typedef for the enhanced memory deallocator function.
 */
typedef void (*cjose_dealloc3_fn_t)(void *, const char *, int);

/**
 * Sets the allocator and deallocator functions.
 *
 * **NOTE:** This function is mutually exclusive from
 * <tt>cjose_set_alloc_ex_funcs()</tt>. Both SHOULD NOT be called.
 *
 * If <tt>alloc</tt> is NULL, any previously set allocator function is cleared
 * and the the default allocator <tt>malloc()</tt>
 * is used.
 *
 * If <tt>dealloc</tt> is NULL, the default dallocator <tt>free()</tt>
 * is used.
 *
 * \param alloc [in] The custom allocator function to use.
 * \param realloc [in] The custom reallocator function to use.
 * \param dealloc [in] The custom deallocator function to use.
 */
void cjose_set_alloc_funcs(cjose_alloc_fn_t alloc, cjose_realloc_fn_t realloc, cjose_dealloc_fn_t dealloc);

/**
 * Sets the enhanced allocator and deallocator functions. This function provides
 * improved support for OpenSSL >= 1.1.x.
 *
 * **NOTE:** This function is mutually exclusive from
 * <tt>cjose_set_alloc_funcs()</tt>. Both SHOULD NOT be called.
 *
 * If <tt>alloc3</tt> is NULL, any previously set allocator function is cleared
 * and the the default allocator <tt>malloc()</tt>
 * is used.
 *
 * If <tt>dealloc3</tt> is NULL, the default dallocator <tt>free()</tt>
 * is used.
 *
 * \param alloc3 [in] The custom allocator function to use for
 *        OpenSSL >= 1.1.0, called with extra file/line params.
 * \param realloc3 [in] The custom reallocator function to use for
 *        OpenSSL >= 1.1.0, called with extra file/line params.
 * \param dealloc3 [in] The custom deallocator function to use for
 *        OpenSSL >= 1.1.0, called with extra file/line params.
 */
void cjose_set_alloc_ex_funcs(cjose_alloc3_fn_t alloc3, cjose_realloc3_fn_t realloc3, cjose_dealloc3_fn_t dealloc3);

/**
 * Retrieves the configured allocator function.  If an allocator function is
 * not set, this function returns a pointer to <tt>malloc()</tt>.
 *
 * \returns The configured allocator function
 */
cjose_alloc_fn_t cjose_get_alloc();

/**
 * Retrieves the configured enhanced allocator function.  If an enhanced
 * allocator function is not set, this function returns a pointer to an
 * internally defined variant that wraps the basic allocator returned by
 * <tt>cjose_get_alloc()</tt>.
 *
 * \returns The configured enhanced allocator function
 */
cjose_alloc3_fn_t cjose_get_alloc3();

/**
 * Retrieve the configured reallocator function. If a reallocator function is
 * not set, this function retursn a pointer to <tt>realloc</tt>.
 *
 * \returns The configured reallocator function
 */
cjose_realloc_fn_t cjose_get_realloc();

/**
 * Retrieves the configured enhanced reallocator function.  If an enhanced
 * reallocator function is not set, this function returns a pointer to an
 * internally defined variant that wraps the basic allocator returned by
 * <tt>cjose_get_realloc()</tt>.
 *
 * \returns The configured enhanced allocator function
 */
cjose_realloc3_fn_t cjose_get_realloc3();

/**
 * Retrieves the configured deallocator function.  If a deallocator function is
 * not set, this function returns a pointer to <tt>free()</tt>.
 *
 * \returns The configured deallocator function
 */
cjose_dealloc_fn_t cjose_get_dealloc();

/**
 * Retrieves the configured enhanced deallocator function.  If an enhanced
 * deallocator function is not set, this function returns a pointer to an
 * internally defined variant that wraps the basic allocator returned by
 * <tt>cjose_get_dealloc()</tt>.
 *
 * \returns The configured enhanced allocator function
 */
cjose_dealloc3_fn_t cjose_get_dealloc3();

/**
 * Compares the first n bytes of the memory areas s1 and s2 in constant time.
 *
 * \param a [in] The first octet string to compare
 * \param b [in] The second octet string to compare
 * \param size [in] The length to compare
 * \returns an  integer  less  than,  equal  to,  or
 *        greater than zero if the first n bytes of s1 is found, respectively, to
 *        be less than, to match, or be greater than the first n bytes of s2
 */
int cjose_const_memcmp(const uint8_t *a, const uint8_t *b, const size_t size);

#ifdef __cplusplus
}
#endif

#endif // CJOSE_UTIL_H

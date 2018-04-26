/**
 * \file
 * \brief
 * Datatypes and functions for error reporting.
 *
 * Copyrights
 *
 * Portions created or assigned to Cisco Systems, Inc. are
 * Copyright (c) 2014-2016 Cisco Systems, Inc.  All Rights Reserved.
 */
#ifndef CJOSE_ERROR_H
#define CJOSE_ERROR_H

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Temporarily disable compiler warnings, if possible (>=gcc-4.6).
 *
 * In some cases (particularly within macros), certain compiler warnings are
 * unavoidable.  In order to allow these warnings to be treated as errors in
 * most cases, these macros will disable particular warnings only during
 * specific points in the compilation.
 */
#if __GNUC__ > 4 || (__GNUC__ == 4 && __GNUC_MINOR__ >= 6)
#define GCC_END_IGNORED_WARNING _Pragma("GCC diagnostic pop")

#define GCC_BEGIN_IGNORED_WARNING_ADDRESS \
    _Pragma("GCC diagnostic push");       \
    _Pragma("GCC diagnostic ignored \"-Waddress\"")
#define GCC_END_IGNORED_WARNING_ADDRESS GCC_END_IGNORED_WARNING
#else
#define GCC_BEGIN_IGNORED_WARNING_ADDRESS
#define GCC_END_IGNORED_WARNING_ADDRESS
#endif /* defined(__GNUC__) && (__GNUC__ > 3) && (__GNUC_MINOR__ > 5) */

/**
 * Enumeration of defined error codes.
 */
typedef enum {
    /** No error */
    CJOSE_ERR_NONE = 0,

    /** argument was invalid (beyond invariants) */
    CJOSE_ERR_INVALID_ARG,

    /** context is not in a valid state */
    CJOSE_ERR_INVALID_STATE,

    /** out of memory */
    CJOSE_ERR_NO_MEMORY,

    /** an error returned from the crypto libraries */
    CJOSE_ERR_CRYPTO,

} cjose_errcode;

/**
 * An instance of an error context. Unlike other structures, it
 * is the API user's responsibility to allocate the structure; however
 * the values provided are considered constants, and MUST NOT be
 * deallocated.
 */
typedef struct
{
    /** The error code */
    cjose_errcode code;

    /** The human readable message for the error code */
    const char *message;

    /** The function where the error occured, or "<unknown>"
        if it cannot be determined */
    const char *function;

    /** The file where the error occured */
    const char *file;

    /** The line number in the file where the error occured */
    unsigned long line;

} cjose_err;

/**
 * Retrieves the error message for the given error code.
 *
 * \param code The error code to lookup
 * \retval const char * The message for {code}
 */
const char *cjose_err_message(cjose_errcode code);

/**
 * \def CJOSE_ERROR(err, code)
 *
 * Macro to initialize an error context.
 *
 * \param err The pointer to the error context, or NULL if none
 * \param errcode The error code
 */
#define CJOSE_ERROR(err, errcode)                      \
    GCC_BEGIN_IGNORED_WARNING_ADDRESS                  \
    if ((err) != NULL && (errcode) != CJOSE_ERR_NONE)  \
    {                                                  \
        (err)->code = (errcode);                       \
        (err)->message = cjose_err_message((errcode)); \
        (err)->function = __func__;                    \
        (err)->file = __FILE__;                        \
        (err)->line = __LINE__;                        \
    }                                                  \
    GCC_END_IGNORED_WARNING_ADDRESS

#ifdef __cplusplus
}
#endif

#endif /* CJOSE_ERROR_H */

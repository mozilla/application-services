/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef fxa_h
#define fxa_h

#include <stdint.h>
#include <Foundation/NSObjCRuntime.h>

/*
 * This file contains headers for all of the structs and functions that map directly to the functions
 * defined in fxa-client/src/ffi.rs, fxa-client/ffi/src/lib.rs, and components/support/ffi/src/error.rs.
 *
 * The C in this file is specifically formatted to be used with Objective C and Swift and contains
 * macros and flags that will not be recognised by other C based languages.
 */

/*
  Error codes reported by the fxa-client library, from fxa-client/src/ffi.rs
 */
enum {
    InternalPanic = -1,
    NoError = 0,
    Other = 1,
    AuthenticationError = 2,
    NetworkError = 3,
};

/*
 A mapping of the ExternError repr(C) Rust struct, from components/support/ffi/src/error.rs.
 */
typedef struct FxAErrorC {
    int32_t code;
    char *_Nullable message;
} FxAErrorC;

/*
 A mapping of the ByteBuffer repr(C) Rust struct, from components/support/ffi/src/lib.rs.
 */
typedef struct ByteBuffer {
    int64_t len;
    uint8_t *_Nullable data;
} ByteBuffer;

typedef struct AccessTokenInfoC {
    const char *const _Nonnull scope;
    const char *const _Nonnull token;
    const char *const _Nullable key;
    const int64_t expires_at;
} AccessTokenInfoC;

typedef struct SyncKeysC {
    const char *const _Nonnull sync_key;
    const char *const _Nonnull xcs;
} SyncKeysC;

typedef struct ProfileC {
    const char *const _Nonnull uid;
    const char *const _Nonnull email;
    const char *const _Nullable avatar;
    const uint8_t avatar_default;
    const char *const _Nullable display_name;
} ProfileC;

typedef uint64_t FirefoxAccountHandle;

char *_Nonnull fxa_begin_oauth_flow(FirefoxAccountHandle handle,
                                    const char *_Nonnull scopes,
                                    bool wants_keys,
                                    FxAErrorC *_Nonnull out);

void fxa_complete_oauth_flow(FirefoxAccountHandle handle,
                             const char *_Nonnull code,
                             const char *_Nonnull state,
                             FxAErrorC *_Nonnull out);

AccessTokenInfoC *_Nullable fxa_get_access_token(FirefoxAccountHandle handle,
                                                 const char *_Nonnull scope,
                                                 FxAErrorC *_Nonnull out);

FirefoxAccountHandle fxa_from_json(const char *_Nonnull json,
                                   FxAErrorC *_Nonnull out);

char *_Nullable fxa_to_json(FirefoxAccountHandle handle,
                            FxAErrorC *_Nonnull out);

void fxa_register_persist_callback(FirefoxAccountHandle handle,
                                   void (*_Nonnull callback_fn)(const char *_Nonnull json),
                                   FxAErrorC *_Nonnull out);

void fxa_unregister_persist_callback(FirefoxAccountHandle handle,
                                     FxAErrorC *_Nonnull out);

FirefoxAccountHandle fxa_new(const char *_Nonnull content_base,
                             const char *_Nonnull client_id,
                             const char *_Nonnull redirect_uri,
                             FxAErrorC *_Nonnull out);

ByteBuffer fxa_profile(FirefoxAccountHandle handle,
                       bool ignore_cache,
                       FxAErrorC *_Nonnull out);

FirefoxAccountHandle fxa_from_credentials(const char *_Nonnull content_base,
                                          const char *_Nonnull client_id,
                                          const char *_Nonnull redirect_uri,
                                          const char *_Nonnull json,
                                          FxAErrorC *_Nonnull out);

char *_Nullable fxa_assertion_new(FirefoxAccountHandle handle,
                                  const char *_Nonnull audience,
                                  FxAErrorC *_Nonnull out);

char *_Nullable fxa_get_token_server_endpoint_url(FirefoxAccountHandle handle,
                                                  FxAErrorC *_Nonnull out);

char *_Nullable fxa_get_connection_success_url(FirefoxAccountHandle handle,
                                               FxAErrorC *_Nonnull out);

SyncKeysC *_Nullable fxa_get_sync_keys(FirefoxAccountHandle handle,
                                       FxAErrorC *_Nonnull out);

void fxa_str_free(char *_Nullable ptr);
void fxa_free(FirefoxAccountHandle h, FxAErrorC *_Nonnull out);
void fxa_oauth_info_free(AccessTokenInfoC *_Nullable ptr);
void fxa_bytebuffer_free(ByteBuffer buffer);
void fxa_sync_keys_free(SyncKeysC *_Nullable ptr);

#endif /* fxa_h */

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef fxa_h
#define fxa_h

#include <stdint.h>
#include <Foundation/NSObjCRuntime.h>

/*
 * This file contains headers for all of the structs and functions that map directly to the functions
 * defined in fxa_rust_client/ffi/src/lib.rs.
 *
 * The C in this file is specifically formatted to be used with Objective C and Swift and contains
 * macros and flags that will not be recognised by other C based languages.
 */

/*
 A mapping of the ErrorCode repr(C) Rust enum.
 */
typedef enum ErrorCode {
    NoError = 0,
    Other = 1,
    AuthenticationError = 2,
    InternalPanic = 3,
} ErrorCode;

/*
 A mapping of the ExternError repr(C) Rust struct.
 */
typedef struct FxAErrorC {
    ErrorCode code;
    char *_Nullable message;
} FxAErrorC;

typedef struct OAuthInfoC {
    const char *const _Nonnull access_token;
    const char *const _Nullable keys;
    const char *const _Nonnull scope;
} OAuthInfoC;

typedef struct SyncKeysC {
    const char *const _Nonnull sync_key;
    const char *const _Nonnull xcs;
} SyncKeysC;

typedef struct ProfileC {
    const char *const _Nonnull uid;
    const char *const _Nonnull email;
    const char *const _Nonnull avatar;
    const char *const _Nullable display_name;
} ProfileC;

typedef struct FirefoxAccount FirefoxAccount;
typedef struct Config Config;

Config *_Nullable fxa_get_release_config(FxAErrorC *_Nonnull out);

Config *_Nullable fxa_get_custom_config(const char *_Nonnull content_base,
                                        FxAErrorC *_Nonnull out);

char *_Nonnull fxa_begin_oauth_flow(FirefoxAccount *_Nonnull fxa,
                                    const char *_Nonnull scopes,
                                    bool wants_keys,
                                    FxAErrorC *_Nonnull out);

OAuthInfoC *_Nullable fxa_complete_oauth_flow(FirefoxAccount *_Nonnull fxa,
                                              const char *_Nonnull code,
                                              const char *_Nonnull state,
                                              FxAErrorC *_Nonnull out);

OAuthInfoC *_Nullable fxa_get_oauth_token(FirefoxAccount *_Nonnull fxa,
                                          const char *_Nonnull scope,
                                          FxAErrorC *_Nonnull out);

FirefoxAccount *_Nullable fxa_from_json(const char *_Nonnull json,
                                        FxAErrorC *_Nonnull out);

char *_Nullable fxa_to_json(FirefoxAccount *_Nonnull fxa,
                            FxAErrorC *_Nonnull out);

void fxa_register_persist_callback(FirefoxAccount *_Nonnull fxa,
                                   void (*_Nonnull callback_fn)(const char* _Nonnull json),
                                   FxAErrorC *_Nonnull out);

void fxa_unregister_persist_callback(FirefoxAccount *_Nonnull fxa,
                                     FxAErrorC *_Nonnull out);

FirefoxAccount *_Nullable fxa_new(Config *_Nonnull config,
                                  const char *_Nonnull client_id,
                                  const char *_Nonnull redirect_uri,
                                  FxAErrorC *_Nonnull out);

ProfileC *_Nullable fxa_profile(FirefoxAccount *_Nonnull fxa,
                                bool ignore_cache,
                                FxAErrorC *_Nonnull out);

FirefoxAccount *_Nullable fxa_from_credentials(Config *_Nonnull config,
                                               const char *_Nonnull client_id,
                                               const char *_Nonnull redirect_uri,
                                               const char *_Nonnull json,
                                               FxAErrorC *_Nonnull out);

char *_Nullable fxa_assertion_new(FirefoxAccount *_Nonnull fxa,
                                  const char *_Nonnull audience,
                                  FxAErrorC *_Nonnull out);

char *_Nullable fxa_get_token_server_endpoint_url(FirefoxAccount *_Nonnull fxa,
                                                  FxAErrorC *_Nonnull out);

SyncKeysC *_Nullable fxa_get_sync_keys(FirefoxAccount *_Nonnull fxa,
                                       FxAErrorC *_Nonnull out);

void fxa_str_free(char* _Nullable ptr);
void fxa_free(FirefoxAccount* _Nullable ptr);
void fxa_oauth_info_free(OAuthInfoC* _Nullable ptr);
void fxa_profile_free(ProfileC* _Nullable ptr);
void fxa_config_free(Config* _Nullable ptr);
void fxa_sync_keys_free(SyncKeysC* _Nullable ptr);

#endif /* fxa_h */

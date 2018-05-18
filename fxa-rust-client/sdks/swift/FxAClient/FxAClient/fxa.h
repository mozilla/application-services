/* Copyright 2018 Mozilla
 *
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */

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
    Other,
    AuthenticationError
} ErrorCode;

/*
 A mapping of the ExternError repr(C) Rust struct.
 */
typedef struct ErrorC {
    ErrorCode code;
    char *_Nonnull message;
} ErrorC;

/*
 A mapping of the ExternResult repr(C) Rust struct.
 */
typedef struct Result {
    void* _Nullable ok; // Might be a nullptr if optional.
    ErrorC *_Nullable err;
} Result;

typedef struct OAuthInfoC {
    char *_Nonnull access_token;
    char *_Nullable keys_jwe;
    char *_Nonnull scope;
} OAuthInfoC;

typedef struct SyncKeysC {
    char *_Nonnull sync_key;
    char *_Nonnull xcs;
} SyncKeysC;

typedef struct ProfileC {
    char *_Nonnull uid;
    char *_Nonnull email;
    char *_Nonnull avatar;
} ProfileC;

typedef struct FirefoxAccount FirefoxAccount;
typedef struct Config Config;

Result*_Nonnull fxa_get_release_config(void);
Result*_Nonnull fxa_get_custom_config(const char *_Nonnull content_base);
Result*_Nonnull fxa_begin_oauth_flow(FirefoxAccount *_Nonnull fxa,
                           const char *_Nonnull redirect_uri,
                           const char *_Nonnull scopes,
                           bool wants_keys);
Result*_Nonnull fxa_complete_oauth_flow(FirefoxAccount *_Nonnull fxa, const char *_Nonnull code, const char *_Nonnull state);
Result*_Nonnull fxa_get_oauth_token(FirefoxAccount *_Nonnull fxa, const char *_Nonnull scope);
Result*_Nonnull fxa_from_json(const char *_Nonnull json);
Result*_Nonnull fxa_to_json(FirefoxAccount *_Nonnull fxa);
Result*_Nonnull fxa_new(Config *config, const char *_Nonnull client_id);
Result*_Nonnull fxa_profile(FirefoxAccount *_Nonnull fxa, const char *_Nonnull profile_access_token, bool ignore_cache);
Result*_Nonnull fxa_from_credentials(Config *_Nonnull config, const char *_Nonnull client_id, const char *_Nonnull json);
Result*_Nonnull fxa_assertion_new(FirefoxAccount *_Nonnull fxa, const char *_Nonnull audience);
char *_Nonnull fxa_get_token_server_endpoint_url(FirefoxAccount *_Nonnull fxa);
Result*_Nonnull fxa_get_sync_keys(FirefoxAccount *_Nonnull fxa);

void free_extern_result(Result* _Nullable ptr);
void free_extern_error(ErrorC* _Nullable ptr);
void fxa_str_free(char* _Nullable ptr);
void fxa_free(FirefoxAccount* _Nullable ptr);
void fxa_oauth_info_free(OAuthInfoC* _Nullable ptr);
void fxa_profile_free(ProfileC* _Nullable ptr);
void fxa_config_free(Config* _Nullable ptr);
void fxa_sync_keys_free(SyncKeysC* _Nullable ptr);

#endif /* fxa_h */

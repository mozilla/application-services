/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
#pragma once

#include <stdint.h>

typedef enum Sync15PasswordsErrorCode {
    Sync15Passwords_OtherError       = -2,
    Sync15Passwords_UnexpectedPanic  = -1,
    Sync15Passwords_NoError          = 0,
    Sync15Passwords_AuthInvalidError = 1,
    Sync15Passwords_NoSuchRecord     = 2,
    Sync15Passwords_DuplicateGuid    = 3,
    Sync15Passwords_InvalidLogin     = 4,
    Sync15Passwords_InvalidKeyError  = 5,
    Sync15Passwords_NetworkError     = 6,
} Sync15PasswordsErrorCode;

typedef struct Sync15PasswordsError {
    Sync15PasswordsErrorCode code;
    char* _Nullable message;
} Sync15PasswordsError;

typedef struct Sync15PasswordEngineRaw Sync15PasswordEngineRaw;

Sync15PasswordEngineRaw* sync15_passwords_state_new(char const* _Nonnull db_path,
                                                    char const* _Nonnull encryption_key,
                                                    Sync15PasswordsError* _Nonnull error_out);

void sync15_passwords_state_destroy(Sync15PasswordEngineRaw* _Nonnull engine);


char* sync15_passwords_get_by_id(Sync15PasswordEngineRaw* _Nonnull engine,
                                 char const* _Nonnull id,
                                 Sync15PasswordsError *_Nonnull error_out);

char* sync15_passwords_get_by_id(Sync15PasswordEngineRaw* _Nonnull engine,
                                 char const* _Nonnull id,
                                 Sync15PasswordsError *_Nonnull error_out);

char* sync15_passwords_get_all(Sync15PasswordEngineRaw* _Nonnull engine,
                               Sync15PasswordsError *_Nonnull error_out);


void sync15_passwords_sync(Sync15PasswordEngineRaw* _Nonnull engine,
                           char const* _Nonnull key_id,
                           char const* _Nonnull access_token,
                           char const* _Nonnull sync_key,
                           char const* _Nonnull token_server_url,
                           Sync15PasswordsError *_Nonnull error);

void sync15_passwords_wipe(Sync15PasswordEngineRaw* _Nonnull engine,
                           Sync15PasswordsError *_Nonnull error);

void sync15_passwords_reset(Sync15PasswordEngineRaw* _Nonnull engine,
                            Sync15PasswordsError *_Nonnull error);

void sync15_passwords_touch(Sync15PasswordEngineRaw* _Nonnull engine,
                            char const* _Nonnull id,
                            Sync15PasswordsError *_Nonnull error);

uint8_t sync15_passwords_delete(Sync15PasswordEngineRaw* _Nonnull engine,
                                char const* _Nonnull id,
                                Sync15PasswordsError *_Nonnull error);

char* sync15_passwords_add(Sync15PasswordEngineRaw* _Nonnull engine,
                           char const* _Nonnull json,
                           Sync15PasswordsError *_Nonnull error);

void sync15_passwords_update(Sync15PasswordEngineRaw* _Nonnull engine,
                             char const* _Nonnull json,
                             Sync15PasswordsError *_Nonnull error);

char* sync15_passwords_destroy_string(char const* _Nonnull str);

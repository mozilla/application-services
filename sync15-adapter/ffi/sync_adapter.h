/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
#ifndef SYNC_ADAPTER_15_H
#define SYNC_ADAPTER_15_H
// size_t
#include <stddef.h>
// int64_t
#include <stdint.h>

typedef struct sync15_PasswordRecord     sync15_PasswordRecord;
typedef struct sync15_PasswordCollection sync15_PasswordCollection;
typedef struct sync15_Service            sync15_Service;

struct sync15_PasswordRecord {
    const char* id;
    // Might be null!
    const char* hostname;
    // Might be null!
    const char* form_submit_url;
    const char* http_realm;

    const char* username;
    const char* password;

    const char* username_field;
    const char* password_field;

    // In ms since unix epoch
    int64_t time_created;

    // In ms since unix epoch
    int64_t time_password_changed;

    // -1 for missing, otherwise in ms_since_unix_epoch
    int64_t time_last_used;

    // -1 for missing
    int64_t times_used;
};

sync15_Service *sync15_service_create(const char* key_id,
                                      const char* access_token,
                                      const char* sync_key,
                                      const char* tokenserver_base_url);

void sync15_service_destroy(sync15_Service* svc);

sync15_PasswordCollection* sync15_service_request_passwords(sync15_Service* svc);
void sync15_passwords_destroy(sync15_PasswordCollection *passwords);

size_t sync15_passwords_record_count(const sync15_PasswordCollection* passwords);
size_t sync15_passwords_tombstone_count(const sync15_PasswordCollection* passwords);

// Caller frees! Returns null if index > sync15_passwords_tombstone_count(passwords)
char *sync15_passwords_get_tombstone_at(const sync15_PasswordCollection* pws, size_t i);

// Caller frees (via sync15_password_record_free) Returns null if index > sync15_passwords_record_count(pws)
sync15_PasswordRecord* sync15_passwords_get_record_at(const sync15_PasswordCollection* pws, size_t i);

void sync15_password_record_destroy(sync15_PasswordRecord *record);

#endif

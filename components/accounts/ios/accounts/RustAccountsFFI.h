/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#pragma once
#include <stdint.h>
#include <Foundation/NSObjCRuntime.h>

// TODO: remove and consolidate once we have a single FFI for FxA.
#import "RustFxAFFI.h"

/*
 * This file contains headers for all of the structs and functions that map directly to the functions
 * defined in accounts/src/ffi.rs, accounts/ffi/src/lib.rs, and components/support/ffi/src/error.rs.
 *
 * The C in this file is specifically formatted to be used with Objective C and Swift and contains
 * macros and flags that will not be recognised by other C based languages.
 */

/*
 A mapping of the ByteBuffer repr(C) Rust struct, from components/support/ffi/src/lib.rs.
 */
typedef struct FxAManagerRustBuffer {
    int64_t len;
    uint8_t *_Nullable data;
} FxAManagerRustBuffer;

typedef uint64_t ManagerHandle;


ManagerHandle fxa_mgr_new(const char *_Nonnull content_base,
                          const char *_Nonnull client_id,
                          const char *_Nonnull redirect_uri,
                          const char *_Nonnull device_name,
                          int32_t device_type,
                          uint8_t const *_Nonnull  capabilities_data,
                          const int32_t capabilities_len,
                          FxAError *_Nonnull out);

void fxa_mgr_init(ManagerHandle handle,
                  const char *_Nullable jsonState,
                  FxAError *_Nonnull out);

FxAManagerRustBuffer fxa_mgr_account_state(ManagerHandle handle,
                                           FxAError *_Nonnull out);

char *_Nullable fxa_mgr_begin_oauth_flow(
    ManagerHandle handle,
    FxAError *_Nonnull out
);

char *_Nullable fxa_mgr_begin_pairing_flow(
    ManagerHandle handle,
    const char *_Nonnull pairingUrl,
    FxAError *_Nonnull out
);

void fxa_mgr_finish_authentication_flow(
    ManagerHandle handle,
    const char *_Nonnull code,
    const char *_Nonnull state,
    FxAError *_Nonnull out
);

void fxa_mgr_on_authentication_error(
    ManagerHandle handle,
    FxAError *_Nonnull out
);

FxAManagerRustBuffer fxa_mgr_get_profile(
    ManagerHandle handle,
    FxAError *_Nonnull out
);

FxAManagerRustBuffer fxa_mgr_update_profile(
    ManagerHandle handle,
    FxAError *_Nonnull out
);

void fxa_mgr_logout(
    ManagerHandle handle,
    FxAError *_Nonnull out
);

char *_Nullable fxa_mgr_export_persisted_state(
    ManagerHandle handle,
    FxAError *_Nonnull out
);

FxAManagerRustBuffer fxa_mgr_update_devices(
    ManagerHandle handle,
    FxAError *_Nonnull out
);

FxAManagerRustBuffer fxa_mgr_get_devices(
    ManagerHandle handle,
    FxAError *_Nonnull out
);

FxAManagerRustBuffer fxa_mgr_handle_push_message(
    ManagerHandle handle,
    const char *_Nonnull jsonPayload,
    FxAError *_Nonnull out
);

void fxa_mgr_set_device_name(
    ManagerHandle handle,
    const char *_Nonnull displayName,
    FxAError *_Nonnull out
);

FxAManagerRustBuffer fxa_mgr_poll_device_commands(
    ManagerHandle handle,
    FxAError *_Nonnull out
);

void fxa_mgr_set_push_subscription(
    ManagerHandle handle,
    const char *_Nonnull endpoint,
    const char *_Nonnull publicKey,
    const char *_Nonnull authKey,
    FxAError *_Nonnull out
);

void fxa_mgr_send_tab(
    ManagerHandle handle,
    const char *_Nonnull targetDeviceId,
    const char *_Nonnull title,
    const char *_Nonnull url,
    FxAError *_Nonnull out
);

void fxa_mgr_str_free(char *_Nullable ptr);
void fxa_mgr_free(ManagerHandle h, FxAError *_Nonnull out);
void fxa_mgr_bytebuffer_free(FxAManagerRustBuffer buffer);

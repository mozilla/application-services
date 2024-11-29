/* -*- Mode: C++; tab-width: 2; indent-tabs-mode: nil; c-basic-offset: 2 -*- */
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "mozilla/Types.h"

// This seems sad?
// If we could get a stub .rs we could use `tabs::uniffi_reexport_scaffolding!();` etc,
// but here we are.
// To get the symbols from our static lib we need to refer to a symbol from each crate within it.
// This is an arbitrary choice - any symbol will do, but we chose these because every uniffi crate has it.
extern int MOZ_EXPORT ffi_autofill_uniffi_contract_version();
extern int MOZ_EXPORT ffi_crashtest_uniffi_contract_version();
extern int MOZ_EXPORT ffi_fxa_client_uniffi_contract_version();
extern int MOZ_EXPORT ffi_init_rust_components_uniffi_contract_version();
extern int MOZ_EXPORT ffi_logins_uniffi_contract_version();
extern int MOZ_EXPORT ffi_merino_uniffi_contract_version();
extern int MOZ_EXPORT ffi_nimbus_uniffi_contract_version();
extern int MOZ_EXPORT ffi_places_uniffi_contract_version();
extern int MOZ_EXPORT ffi_push_uniffi_contract_version();
extern int MOZ_EXPORT ffi_relay_uniffi_contract_version();
extern int MOZ_EXPORT ffi_remote_settings_uniffi_contract_version();
extern int MOZ_EXPORT ffi_rust_log_forwarder_uniffi_contract_version();
extern int MOZ_EXPORT ffi_search_uniffi_contract_version();
extern int MOZ_EXPORT ffi_suggest_uniffi_contract_version();
extern int MOZ_EXPORT ffi_sync15_uniffi_contract_version();
extern int MOZ_EXPORT ffi_sync_manager_uniffi_contract_version();
extern int MOZ_EXPORT ffi_tabs_uniffi_contract_version();

// far out, this is crazy - without this, only the search _NAMESPACE meta comes in,
// meaning uniffi ends up generating a completely empty kotlin module for search.
// Looking at `nm obj-dir/.../libsearch-*.rlib` you can see many symbols
// are in a different .o - this symbol is one taken randomly from the .o with
// the missing symbols, and they all happily come in.
// W T A F.
extern int MOZ_EXPORT uniffi_search_checksum_constructor_searchengineselector_new();

void _local_megazord_dummy_symbol() {
    ffi_autofill_uniffi_contract_version();
    ffi_crashtest_uniffi_contract_version();
    ffi_fxa_client_uniffi_contract_version();
    ffi_init_rust_components_uniffi_contract_version();
    ffi_logins_uniffi_contract_version();
    ffi_merino_uniffi_contract_version();
    ffi_nimbus_uniffi_contract_version();
    ffi_places_uniffi_contract_version();
    ffi_push_uniffi_contract_version();
    ffi_relay_uniffi_contract_version();
    ffi_remote_settings_uniffi_contract_version();
    ffi_rust_log_forwarder_uniffi_contract_version();
    ffi_search_uniffi_contract_version();
    ffi_suggest_uniffi_contract_version();
    ffi_sync15_uniffi_contract_version();
    ffi_sync_manager_uniffi_contract_version();
    ffi_tabs_uniffi_contract_version();
    uniffi_search_checksum_constructor_searchengineselector_new();
}

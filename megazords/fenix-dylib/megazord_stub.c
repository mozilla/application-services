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
extern int MOZ_EXPORT ffi_ads_client_uniffi_contract_version();
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
// Same deal for these, but their .o has no checksum fn, so we name the metadata directly.
extern int MOZ_EXPORT UNIFFI_META_ADS_CLIENT_ERROR_MOZADSCLIENTAPIERROR();
extern int MOZ_EXPORT UNIFFI_META_MERINO_CONSTRUCTOR_CURATEDRECOMMENDATIONSCLIENT_NEW();
extern int MOZ_EXPORT UNIFFI_META_MERINO_CONSTRUCTOR_SUGGESTCLIENT_NEW();
extern int MOZ_EXPORT UNIFFI_META_MERINO_CONSTRUCTOR_WORLDCUPCLIENT_NEW();
extern int MOZ_EXPORT UNIFFI_META_MERINO_ENUM_CURATEDRECOMMENDATIONLOCALE();
extern int MOZ_EXPORT UNIFFI_META_MERINO_ERROR_CURATEDRECOMMENDATIONSAPIERROR();
extern int MOZ_EXPORT UNIFFI_META_MERINO_ERROR_MERINOSUGGESTAPIERROR();
extern int MOZ_EXPORT UNIFFI_META_MERINO_ERROR_MERINOWORLDCUPAPIERROR();
extern int MOZ_EXPORT UNIFFI_META_MERINO_FUNC_ALL_CURATED_RECOMMENDATION_LOCALES();
extern int MOZ_EXPORT UNIFFI_META_MERINO_FUNC_CURATED_RECOMMENDATION_LOCALE_FROM_STRING();
extern int MOZ_EXPORT UNIFFI_META_MERINO_INTERFACE_CURATEDRECOMMENDATIONSCLIENT();
extern int MOZ_EXPORT UNIFFI_META_MERINO_INTERFACE_SUGGESTCLIENT();
extern int MOZ_EXPORT UNIFFI_META_MERINO_INTERFACE_WORLDCUPCLIENT();
extern int MOZ_EXPORT UNIFFI_META_MERINO_METHOD_CURATEDRECOMMENDATIONSCLIENT_GET_CURATED_RECOMMENDATIONS();
extern int MOZ_EXPORT UNIFFI_META_MERINO_METHOD_SUGGESTCLIENT_GET_SUGGESTIONS();
extern int MOZ_EXPORT UNIFFI_META_MERINO_METHOD_WORLDCUPCLIENT_GET_LIVE();
extern int MOZ_EXPORT UNIFFI_META_MERINO_METHOD_WORLDCUPCLIENT_GET_MATCHES();
extern int MOZ_EXPORT UNIFFI_META_MERINO_METHOD_WORLDCUPCLIENT_GET_TEAMS();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_CURATEDRECOMMENDATIONSCONFIG();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_CURATEDRECOMMENDATIONSREQUEST();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_CURATEDRECOMMENDATIONSRESPONSE();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_FEEDSECTION();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_INTERESTPICKER();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_INTERESTPICKERSECTION();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_LAYOUT();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_RECOMMENDATIONDATAITEM();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_RESPONSIVELAYOUT();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_SECTIONSETTINGS();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_SUGGESTCONFIG();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_SUGGESTOPTIONS();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_TILE();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_WORLDCUPCONFIG();
extern int MOZ_EXPORT UNIFFI_META_MERINO_RECORD_WORLDCUPOPTIONS();
extern int MOZ_EXPORT UNIFFI_META_NAMESPACE_ERRORSUPPORT();
extern int MOZ_EXPORT UNIFFI_META_SYNC15_ENUM_DEVICETYPE();

void _local_megazord_dummy_symbol() {
    ffi_ads_client_uniffi_contract_version();
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
    UNIFFI_META_ADS_CLIENT_ERROR_MOZADSCLIENTAPIERROR();
    UNIFFI_META_MERINO_CONSTRUCTOR_CURATEDRECOMMENDATIONSCLIENT_NEW();
    UNIFFI_META_MERINO_CONSTRUCTOR_SUGGESTCLIENT_NEW();
    UNIFFI_META_MERINO_CONSTRUCTOR_WORLDCUPCLIENT_NEW();
    UNIFFI_META_MERINO_ENUM_CURATEDRECOMMENDATIONLOCALE();
    UNIFFI_META_MERINO_ERROR_CURATEDRECOMMENDATIONSAPIERROR();
    UNIFFI_META_MERINO_ERROR_MERINOSUGGESTAPIERROR();
    UNIFFI_META_MERINO_ERROR_MERINOWORLDCUPAPIERROR();
    UNIFFI_META_MERINO_FUNC_ALL_CURATED_RECOMMENDATION_LOCALES();
    UNIFFI_META_MERINO_FUNC_CURATED_RECOMMENDATION_LOCALE_FROM_STRING();
    UNIFFI_META_MERINO_INTERFACE_CURATEDRECOMMENDATIONSCLIENT();
    UNIFFI_META_MERINO_INTERFACE_SUGGESTCLIENT();
    UNIFFI_META_MERINO_INTERFACE_WORLDCUPCLIENT();
    UNIFFI_META_MERINO_METHOD_CURATEDRECOMMENDATIONSCLIENT_GET_CURATED_RECOMMENDATIONS();
    UNIFFI_META_MERINO_METHOD_SUGGESTCLIENT_GET_SUGGESTIONS();
    UNIFFI_META_MERINO_METHOD_WORLDCUPCLIENT_GET_LIVE();
    UNIFFI_META_MERINO_METHOD_WORLDCUPCLIENT_GET_MATCHES();
    UNIFFI_META_MERINO_METHOD_WORLDCUPCLIENT_GET_TEAMS();
    UNIFFI_META_MERINO_RECORD_CURATEDRECOMMENDATIONSCONFIG();
    UNIFFI_META_MERINO_RECORD_CURATEDRECOMMENDATIONSREQUEST();
    UNIFFI_META_MERINO_RECORD_CURATEDRECOMMENDATIONSRESPONSE();
    UNIFFI_META_MERINO_RECORD_FEEDSECTION();
    UNIFFI_META_MERINO_RECORD_INTERESTPICKER();
    UNIFFI_META_MERINO_RECORD_INTERESTPICKERSECTION();
    UNIFFI_META_MERINO_RECORD_LAYOUT();
    UNIFFI_META_MERINO_RECORD_RECOMMENDATIONDATAITEM();
    UNIFFI_META_MERINO_RECORD_RESPONSIVELAYOUT();
    UNIFFI_META_MERINO_RECORD_SECTIONSETTINGS();
    UNIFFI_META_MERINO_RECORD_SUGGESTCONFIG();
    UNIFFI_META_MERINO_RECORD_SUGGESTOPTIONS();
    UNIFFI_META_MERINO_RECORD_TILE();
    UNIFFI_META_MERINO_RECORD_WORLDCUPCONFIG();
    UNIFFI_META_MERINO_RECORD_WORLDCUPOPTIONS();
    UNIFFI_META_NAMESPACE_ERRORSUPPORT();
    UNIFFI_META_SYNC15_ENUM_DEVICETYPE();
}

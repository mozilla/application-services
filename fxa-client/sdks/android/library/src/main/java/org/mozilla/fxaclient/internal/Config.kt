/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.fxaclient.internal

/**
 * Config represents the server endpoint configurations needed for the
 * authentication flow.
 */
class Config internal constructor(rawPointer: RawConfig) : RustObject<RawConfig>(rawPointer) {

    override fun destroy(p: RawConfig) {
        // We're already synchronized by RustObject
        FxaClient.INSTANCE.fxa_config_free(p)
    }

    companion object {
        /**
         * Set up endpoints used in the production Firefox Accounts instance.
         *
         * This performs network requests, and should not be used on the main thread.
         */
        fun release(): Config {
            return Config(unlockedRustCall { e ->
                FxaClient.INSTANCE.fxa_get_release_config(e)
            })
        }

        /**
         * Set up endpoints used by a custom host for authentication
         *
         * This performs network requests, and should not be used on the main thread.
         *
         * @param content_base Hostname of the FxA auth service provider
         */
        fun custom(content_base: String): Config {
            return Config(unlockedRustCall { e ->
                FxaClient.INSTANCE.fxa_get_custom_config(content_base, e)
            })
        }
    }
}

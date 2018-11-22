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
         *
         * @param clientId Client Id of the FxA relier
         * @param redirectUri Redirect Uri of the FxA relier
         */
        fun release(clientId: String, redirectUri: String): Config {
            return Config(unlockedRustCall { e ->
                FxaClient.INSTANCE.fxa_get_release_config(clientId, redirectUri, e)
            })
        }

        /**
         * Set up endpoints used by a custom host for authentication
         *
         * This performs network requests, and should not be used on the main thread.
         *
         * @param contentBase Hostname of the FxA auth service provider
         * @param clientId Client Id of the FxA relier
         * @param redirectUri Redirect Uri of the FxA relier
         */
        fun custom(contentBase: String, clientId: String, redirectUri: String): Config {
            return Config(unlockedRustCall { e ->
                FxaClient.INSTANCE.fxa_get_custom_config(contentBase, clientId, redirectUri, e)
            })
        }
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient.testing

import mozilla.appservices.fxaclient.Config
import mozilla.appservices.fxaclient.MsgTypes
import mozilla.appservices.fxaclient.rust.LibFxAFFI
import mozilla.appservices.fxaclient.rustCall

/**
 * A test helper for live accounts testing:
 * - Creating an instance of `TempTestAccount` will create an account.
 * - When `TempTestAccount` goes out of scope the account will be destroyed.
 */
class TempTestAccount(private val config: Config, val email: String, val password: String): AutoCloseable {
    companion object {
        fun create(config: Config): TempTestAccount {
            val buffer = rustCall { e ->
                LibFxAFFI.INSTANCE.fxa_testing_create_temp_account(
                    config.contentUrl,
                    config.clientId,
                    config.redirectUri,
                    config.tokenServerUrlOverride,
                    e
                )
            }
            try {
                val msg = MsgTypes.TempAccountDetails.parseFrom(buffer.asCodedInputStream()!!)
                return TempTestAccount(config, msg.email, msg.password)
            } finally {
                LibFxAFFI.INSTANCE.fxa_bytebuffer_free(buffer)
            }
        }
    }

    override fun close() {
        rustCall { e ->
            LibFxAFFI.INSTANCE.fxa_testing_destroy_temp_account(
                config.contentUrl,
                config.clientId,
                config.redirectUri,
                config.tokenServerUrlOverride,
                email,
                password,
                e
            )
        }
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.push

import com.sun.jna.Pointer
import java.util.concurrent.atomic.AtomicLong
import org.json.JSONObject
import org.json.JSONArray

import mozilla.appservices.support.RustBuffer


/**
 * An implementation of a [PushAPI] backed by a Rust Push library.
 *
 * @param server_host the host name for the service (e.g. "push.service.mozilla.org").
 * @param socket_protocol the optional socket protocol (default: "https")
 * @param bridge_type the optional bridge protocol (default: "fcm")
 * @param registration_id the native OS messaging registration id
 */
class PushManager(
    sender_id: String,
    server_host: String = "push.service.mozilla.com",
    socket_protocol: String = "https",
    bridge_type: BridgeTypes,
    registration_id: String,
    database_path: String = "push.sqlite") : PushAPI, AutoCloseable {

    private var handle: AtomicLong = AtomicLong(0)

    init {
        try {
	    handle.set(rustCall { error ->
                LibPushFFI.INSTANCE.push_connection_new(
                        server_host,
                        socket_protocol,
                        bridge_type.toString(),
                        registration_id,
                        sender_id,
                        database_path,
                        error)
            })
        } catch (e: InternalPanic) {
            // Do local error handling?

            throw e
        }
    }

    @Synchronized
    override fun close() {
        val handle = this.handle.getAndSet(0L)
        if (handle != 0L) {
            rustCall { error ->
		LibPushFFI.INSTANCE.push_connection_destroy(handle, error)
            }
        }
    }

    override fun subscribe(
        channelID: String,
        scope: String
    ): SubscriptionInfo {
        val json = rustCallForString { error ->
            LibPushFFI.INSTANCE.push_subscribe(
                this.handle.get(), channelID, scope, error)
        }

        return SubscriptionInfo.fromString(json)
    }

    override fun unsubscribe(channelID: String): Boolean {
        return rustCall { error ->
            LibPushFFI.INSTANCE.push_unsubscribe(
                this.handle.get(), channelID, error)
        }.toInt() == 1
    }

    override fun update(registrationToken: String): Boolean {
        return rustCall { error ->
            LibPushFFI.INSTANCE.push_update(
                this.handle.get(), registrationToken, error)
        }.toInt() == 1
    }


    override fun verifyConnection(): Map<String, String> {
        val newEndpoints: MutableMap<String, String> = linkedMapOf()
        val response = rustCallForString { error ->
            LibPushFFI.INSTANCE.push_verify_connection(
                this.handle.get(), error)
        }
        if (response.isNotEmpty()) {
            val visited = JSONObject(response)
            for (key in visited.keys()) {
                newEndpoints[key] = visited[key] as String
            }
        }
        return newEndpoints
    }

    override fun decrypt(
        channelID: String,
        body: String,
        encoding: String,
        salt: String,
        dh: String): ByteArray {
            val result = rustCallForString{ error ->
            LibPushFFI.INSTANCE.push_decrypt(
                this.handle.get(), channelID, body, encoding, salt, dh, error
            )}
            val jarray = JSONArray(result)
            val retarray = ByteArray(jarray.length())
            // `for` is inclusive.
            val end = jarray.length()-1
            for (i in 0 .. end) {
                retarray[i] = jarray.getInt(i).toByte()
            }
            return retarray
        }

    private inline fun <U> rustCall(callback: (RustError.ByReference) -> U): U {
        synchronized(this) {
            val e = RustError.ByReference()
            val ret: U = callback(e)
            if (e.isFailure()) {
                throw e.intoException()
            } else {
                return ret
            }
        }
    }

    private inline fun rustCallForString(callback: (RustError.ByReference) -> Pointer?): String {
        val cstring = rustCall(callback)
                ?: throw RuntimeException("Bug: Don't use this function when you can return" +
                        " null on success.")
        try {
            return cstring.getString(0, "utf8")
        } finally {
            LibPushFFI.INSTANCE.push_destroy_string(cstring)
        }
    }

    private inline fun rustCallForBuffer(callback: (RustError.ByReference) -> RustBuffer.ByValue?): ByteArray {
        val cbuff = rustCall(callback)
                ?: throw RuntimeException("Bug: Don't use this function when you can return" +
                "null on success.")
        try {
            return cbuff.pointer.getByteArray(0, cbuff.size())
        } finally {
            LibPushFFI.INSTANCE.push_destroy_buffer(cbuff)
        }
    }
}

/** The types of supported native bridges.
 *
 * FCM = Google Android Firebase Cloud Messaging
 * ADM = Amazon Device Messaging for FireTV
 * APNS = Apple Push Notification System for iOS
 *
 * Please contact services back-end for any additional bridge protocols.
 */

enum class BridgeTypes {
    FCM, ADM, APNS, TEST;

    override fun toString() = name.toLowerCase()
}

/**
 * A class for providing the auth-related information needed to sync.
 * Note that this has the same shape as `SyncUnlockInfo` from logins - we
 * probably want a way of sharing these.
 */

class KeyInfo (
    var auth: String,
    var p256dh: String
)

class SubscriptionInfo constructor (
        val endpoint:String,
        val keys: KeyInfo)
{


    companion object {
        internal fun fromString(msg: String) : SubscriptionInfo {
            val obj = JSONObject(msg)
            val keyObj = obj.getJSONObject("keys")
            return SubscriptionInfo(
                    endpoint = obj.getString("endpoint"),
                    keys = KeyInfo(
                            auth = keyObj.getString("auth"),
                            p256dh = keyObj.getString("p256dh"))
            )
        }
    }
 }

/**
 * An API for interacting with Push.
 */
interface PushAPI {
    /**
     * Get the Subscription Info block
     *
     * @param channelID Channel ID (UUID) for new subscription
     * @return a Subscription Info structure
     */
    fun subscribe(
        channelID: String,
        scope: String
    ): SubscriptionInfo

    /**
     * Unsubscribe a given channelID
     *
     * @param channelID Channel ID (UUID) for subscription to remove.
     * @return bool.
     */
    fun unsubscribe(channelID: String): Boolean

    /**
     * Updates the Native OS push registration ID.
     * NOTE: if this returns false, the subsequent `verifyConnection()` may result in new
     * endpoint registrations.
     *
     * @param registrationToken the new Native OS push registration ID.
     * @return bool
     */
    fun update(registrationToken: String): Boolean

    /**
     * Verifies the connection state. NOTE: If the internal check fails,
     * endpoints will be re-registered and new endpoints will be returned for
     * known ChannelIDs
     *
     * @return Map of ChannelID: Endpoint, be sure to notify apps registered to given channelIDs of the new Endpoint.
     */
    fun verifyConnection(): Map<String, String>

    /**
     * Decrypts a raw push message.
     *
     * This accepts the content of a Push Message (from websocket or via Native Push systems).
     * for example:
     * ```
     * val decryptedMessage = manager.decrypt(
     *  channelID=message["chid"],
     *  body=message["body"],
     *  encoding=message["con"],
     *  salt=message.getOrElse("enc", ""),
     *  dh=message.getOrElse("crypto-key", "")
     * )
     *
     *
     * @param channelID: the ChannelID (included in the envelope of the message)
     * @param body: The encrypted body of the message
     * @param encoding: The Content Encoding "enc" field of the message
     * @param salt: The "salt" field (if present in the raw message)
     * @param dh: the "dh" field (if present in the raw message)
     * @return Decrypted message body.
     */
    fun decrypt(
                channelID: String,
                body: String,
                encoding: String,
                salt: String,
                dh: String): ByteArray

}

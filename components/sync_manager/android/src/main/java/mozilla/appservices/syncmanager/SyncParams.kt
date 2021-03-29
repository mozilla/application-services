/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.syncmanager

/**
 * Reason for syncing.
 */
enum class SyncReason {
    /**
     * This is a scheduled sync
     */
    SCHEDULED,
    /**
     * This is a manually triggered sync invoked by the user.
     */
    USER,
    /**
     * This is a sync that is running optimistically before
     * the device goes to sleep / is backgrounded.
     */
    PRE_SLEEP,
    /**
     * This is a sync that is run on application startup.
     */
    STARTUP,
    /**
     * This is a sync that is being performed simply to update the
     * enabled state of one or more engines.
     */
    ENABLED_CHANGE,
}

/**
 * A class for providing the auth-related information needed to sync.
 */
data class SyncAuthInfo(
    val kid: String,
    val fxaAccessToken: String,
    val syncKey: String,
    val tokenserverURL: String
)

/**
 * The type of this device. This is used in the UI; for example, to show an
 * icon for this device in the Synced Tabs or Send Tab views in other products.
 */
enum class DeviceType {
    DESKTOP,
    MOBILE,
    TABLET,
    VR,
    TV,
}

/**
 * A class for providing information about this device for syncing.
 */
data class DeviceSettings(
    val fxaDeviceId: String,
    val name: String,
    val type: DeviceType
)

/**
 * Parameters to use for syncing.
 */
data class SyncParams(
    /**
     * The reason we're syncing.
     */
    val reason: SyncReason,
    /**
     * The list of engines to sync.
     *
     * Engine names are lowercase, and refer to the server-side engine name, e.g.
     * "passwords" (not "logins"!), "bookmarks", "history", etc.
     *
     * Requesting that we sync an unknown engine type will result in a
     * [UnsupportedEngine] error.
     *
     * Passing `null` here is used to indicate that all known and configured engines
     * should be synced.
     */
    val engines: List<String>?,

    /**
     * A map of engine name to new-enabled-state. That is,
     *
     * - The map should be empty to indicate "no changes"
     *
     * - The map should have `enginename: true` if an engine named
     *   `enginename` should be enabled.
     *
     * - The map should have `enginename: false` if an engine named
     *   `enginename` should be disabled.
     */
    val enabledChanges: Map<String, Boolean>,

    /**
     * A map of local encryption keys to use for engines. Only some engines
     * use local encryption, so it's expected that many engines will not have
     * entries. If an engine that requires a key does not find one, it will
     * fail to sync.
     *
     * Note that this is for local encryption in the local databases and is not
     * at all related to Sync encryption. The keys are needed to sync because
     * sync will need to decrypt records from the local store before
     * re-encrypting them using sync encryption to store on the server, and
     * vice-versa, records read from the server will be decrypted using sync
     * encryption and then encrypted before being stored locally.
     *
     * The keys of the map are the engine names as used by all other maps
     * and lists here. The value is a string "key" which will have been
     * previously obtained from the engine directly.
     */
    val localEncryptionKeys: Map<String, String>,

    /**
     * The information used to authenticate with the sync server.
     */
    val authInfo: SyncAuthInfo,

    /**
     * The previously persisted sync state (from `SyncResult.persistedState`),
     * if any exists.
     */
    val persistedState: String?,

    /**
     * The information used to populate a client record for this device.
     */
    val deviceSettings: DeviceSettings
) {
    @Suppress("ComplexMethod")
    internal fun toProtobuf(): MsgTypes.SyncParams {
        val builder = MsgTypes.SyncParams.newBuilder()

        this.engines?.let {
            builder.addAllEnginesToSync(it)
            builder.syncAllEngines = false
        } ?: run {
            // Null `engines`, sync everything.
            builder.syncAllEngines = true
        }

        builder.reason = when (this.reason) {
            SyncReason.SCHEDULED -> MsgTypes.SyncReason.SCHEDULED
            SyncReason.USER -> MsgTypes.SyncReason.USER
            SyncReason.PRE_SLEEP -> MsgTypes.SyncReason.PRE_SLEEP
            SyncReason.STARTUP -> MsgTypes.SyncReason.STARTUP
            SyncReason.ENABLED_CHANGE -> MsgTypes.SyncReason.ENABLED_CHANGE
        }

        builder.putAllEnginesToChangeState(this.enabledChanges)
        builder.putAllLocalEncryptionKeys(this.localEncryptionKeys)

        builder.acctAccessToken = this.authInfo.fxaAccessToken
        builder.acctSyncKey = this.authInfo.syncKey
        builder.acctKeyId = this.authInfo.kid
        builder.acctTokenserverUrl = this.authInfo.tokenserverURL
        this.persistedState?.let { builder.persistedState = it }

        builder.fxaDeviceId = this.deviceSettings.fxaDeviceId
        builder.deviceName = this.deviceSettings.name
        builder.deviceType = when (this.deviceSettings.type) {
            DeviceType.DESKTOP -> MsgTypes.DeviceType.DESKTOP
            DeviceType.MOBILE -> MsgTypes.DeviceType.MOBILE
            DeviceType.TABLET -> MsgTypes.DeviceType.TABLET
            DeviceType.VR -> MsgTypes.DeviceType.VR
            DeviceType.TV -> MsgTypes.DeviceType.TV
        }

        return builder.build()
    }
}

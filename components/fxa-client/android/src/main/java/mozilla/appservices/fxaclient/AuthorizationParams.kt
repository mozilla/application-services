/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

data class AuthorizationParams(
    val clientId: String,
    val scopes: Array<String>,
    val state: String,
    val accessType: String = "online",
    val pkceParams: AuthorizationPKCEParams? = null,
    val keysJwk: String? = null
) {
    fun intoMessage(): MsgTypes.AuthorizationParams {
        var b = MsgTypes.AuthorizationParams.newBuilder()
                .setClientId(this.clientId)
                .setScope(this.scopes.joinToString(" "))
                .setState(this.state)
                .setAccessType(this.accessType)
        if (this.pkceParams != null)
            b = b.setPkceParams(this.pkceParams.intoMessage())
        if (this.keysJwk != null)
            b = b.setKeysJwk(this.keysJwk)
        return b.build()
    }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (javaClass != other?.javaClass) return false

        other as AuthorizationParams

        if (clientId != other.clientId) return false
        if (!scopes.contentEquals(other.scopes)) return false
        if (state != other.state) return false
        if (accessType != other.accessType) return false
        if (pkceParams != other.pkceParams) return false
        if (keysJwk != other.keysJwk) return false

        return true
    }

    override fun hashCode(): Int {
        var result = clientId.hashCode()
        result = 31 * result + scopes.contentHashCode()
        result = 31 * result + state.hashCode()
        result = 31 * result + accessType.hashCode()
        result = 31 * result + (pkceParams?.hashCode() ?: 0)
        result = 31 * result + (keysJwk?.hashCode() ?: 0)
        return result
    }
}

data class AuthorizationPKCEParams(
    val codeChallenge: String,
    val codeChallengeMethod: String = "S256"
) {
    fun intoMessage(): MsgTypes.AuthorizationPKCEParams {
        return MsgTypes.AuthorizationPKCEParams.newBuilder()
                .setCodeChallenge(this.codeChallenge)
                .setCodeChallengeMethod(this.codeChallengeMethod)
                .build()
    }
}

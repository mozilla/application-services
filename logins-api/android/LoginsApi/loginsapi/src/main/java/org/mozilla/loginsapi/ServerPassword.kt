package org.mozilla.loginsapi

// TODO: better types (eg, uuid for id? Time-specific fields? etc)
class ServerPassword (
    val id: String,

    val hostname: String,
    val username: String?,
    val password: String,

    // either one of httpReal or formSubmitURL will be non-null, but not both.
    val httpRealm: String? = null,
    val formSubmitURL: String? = null,

    val timesUsed: Int,

    val timeCreated: Long,

    val timeLastUsed: Long,

    val timePasswordChanged: Long,

    val usernameField: String? = null,
    val passwordField: String? = null
)

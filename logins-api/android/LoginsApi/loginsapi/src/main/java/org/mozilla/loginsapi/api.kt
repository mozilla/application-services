package org.mozilla.loginsapi

import org.mozilla.loginsapi.rust.JNA
import android.util.Log
import com.beust.klaxon.Klaxon
import com.beust.klaxon.Parser
import com.sun.jna.Pointer
import org.mozilla.loginsapi.rust.RustError
import java.io.Closeable


class Api {
    companion object {
        init {
            System.loadLibrary("crypto")
            System.loadLibrary("ssl")
            System.loadLibrary("sqlcipher")
            System.loadLibrary("loginsapi_ffi")
        }
        fun createLoginsStore(databasePath: String,
                              metadataPath: String,
                              databaseKey: String,
                              kid: String,
                              accessToken: String,
                              syncKey: String,
                              tokenserverURL: String): LoginsStore {
            Log.d("API", "in the module")
            val rawStore = LoginsStore.withErrorCheck { error ->
                JNA.INSTANCE.sync15_logins_state_new(
                        databasePath,
                        metadataPath,
                        databaseKey,
                        kid,
                        accessToken,
                        syncKey,
                        tokenserverURL,
                        error
                )
            }
            return LoginsStore(rawStore)
        }
    }
}

class RustException(msg: String): Exception(msg) {}

class LoginsStore(private var raw: JNA.RawLoginSyncState?) : Closeable {

    fun sync() {
        Log.d("LoginsAPI", "sync")
        withErrorCheck { JNA.INSTANCE.sync15_logins_sync(this.raw!!, it) }
    }

    fun reset() {
        Log.d("LoginsAPI", "reset")
        withErrorCheck { JNA.INSTANCE.sync15_logins_reset(this.raw!!, it) }
    }

    fun wipe() {
        Log.d("LoginsAPI", "wipe")
        withErrorCheck { JNA.INSTANCE.sync15_logins_wipe(this.raw!!, it) }
    }

    fun delete(id: String) {
        Log.d("LoginsAPI", "delete by id")
        withErrorCheck { error ->
            JNA.INSTANCE.sync15_logins_delete(this.raw!!, id, error)
        }
    }

    fun get(id: String): ServerPassword? {
        val json = withErrorCheckedString { error ->
            JNA.INSTANCE.sync15_logins_get_by_id(this.raw!!, id, error)
        } ?: return null
        return Klaxon().parse<ServerPassword>(json)!!
    }

    fun touch(id: String) {
        Log.d("LoginsAPI", "touch by id")
        withErrorCheck { error ->
            JNA.INSTANCE.sync15_logins_touch(this.raw!!, id, error)
        }
    }

    fun list(): List<ServerPassword> {
        Log.d("LoginsAPI", "list all")
        val json = withErrorCheckedString {
            JNA.INSTANCE.sync15_logins_get_all(this.raw!!, it)
        }!!
        Log.d("Logins", "got list: " + json);
        return Klaxon().parseArray<ServerPassword>(json)!!
    }

    override fun close() {
        if (this.raw != null) {
            JNA.INSTANCE.sync15_logins_state_destroy(this.raw)
            this.raw = null
        }
    }

    // This says it's unused but apparently this is how you add a finalizer in kotlin.
    // No override or anything
    fun finalize() {
        this.close()
    }

    companion object {
        fun getAndConsumeString(p: Pointer?): String? {
            if (p == null) {
                return null;
            }
            try {
                return p.getString(0, "utf8");
            } finally {
                JNA.INSTANCE.destroy_c_char(p);
            }
        }

        fun <T> withErrorCheck(callback: (RustError.ByReference) -> T): T {
            val error = RustError.ByReference();
            val result = callback(error);
            if (error.isFailure) {
                Log.e("LoginsAPI", "Call failed!");
                throw RustException(error.consumeErrorMessage());
            }
            return result;
        }

        fun withErrorCheckedString(callback: (RustError.ByReference) -> Pointer?): String? {
            return getAndConsumeString(withErrorCheck { error -> callback(error) })
        }
    }
}

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

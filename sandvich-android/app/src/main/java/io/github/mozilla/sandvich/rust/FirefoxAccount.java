package io.github.mozilla.sandvich.rust;

import android.text.TextUtils;
import android.util.Log;

import com.sun.jna.Pointer;

public class FirefoxAccount extends RustObject {

    public FirefoxAccount(Pointer pointer) {
        this.rawPointer = pointer;
    }

    /* TODO: This throws a runtime exception if the fxa_new command doesn't complete for whatever
     *       reason (eg. no internet, no whatever). Ask Christian what to do with the super.
     */
    public FirefoxAccount(Config config, String clientId) {
        RustResult result = JNA.INSTANCE.fxa_new(config.rawPointer, clientId);
        config.rawPointer = null;
        if (result.isSuccess()) {
            Pointer ptr = result.ok;
            result.ok = null;
            this.rawPointer = ptr;
        } else {
            Log.e("FirefoxAccount.init", result.getError().message);
            this.rawPointer = null;
        }
    }

    @Override
    public void close() {
        if (this.rawPointer != null) {
            JNA.INSTANCE.fxa_free(this.rawPointer);
        }
    }

    public static FirefoxAccount from(Config config, String clientId, String webChannelResponse) {
        RustResult result = JNA.INSTANCE.fxa_from_credentials(config.rawPointer, clientId, webChannelResponse);
        config.rawPointer = null;
        if (result.isSuccess()) {
            Pointer ptr = result.ok;
            result.ok = null;
            return new FirefoxAccount(ptr);
        } else {
            Log.e("fxa.from", result.getError().message);
            return null;
        }
    }

    public String beginOAuthFlow(String redirectURI, String[] scopes, Boolean wantsKeys) {
        String scope = TextUtils.join(" ", scopes);
        RustResult result = JNA.INSTANCE.fxa_begin_oauth_flow(this.rawPointer, redirectURI, scope, wantsKeys);
        if (result.isSuccess()) {
            Pointer ptr = result.ok;
            result.ok = null;
            return ptr.getString(0, "utf8");
        } else {
            Log.e("fxa.beginOAuthFlow", result.getError().message);
            return null;
        }
    }
}

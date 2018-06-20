package io.github.mozilla.sandvich.rust;

import android.text.TextUtils;
import android.util.Log;

import com.sun.jna.Pointer;

public class FirefoxAccount extends RustObject {

    public FirefoxAccount(Pointer pointer) {
        super(pointer);
    }

    /* TODO: This throws a runtime exception if the fxa_new command doesn't complete for whatever
     *       reason (eg. no internet, no whatever). Ask Christian what to do with the super.
     */
    public FirefoxAccount(Config config, String clientId) {
        super(JNA.INSTANCE.fxa_new(config.validPointer(), clientId).consumeSuccess());
    }

    @Override
    protected void destroyPointer(Pointer fxa) {
        JNA.INSTANCE.fxa_free(fxa);
    }

    public static FirefoxAccount from(Config config, String clientId, String webChannelResponse) {
        RustResult result = JNA.INSTANCE.fxa_from_credentials(config.validPointer(), clientId, webChannelResponse);
        result.logIfFailure("FirefoxAccount.from");
        if (result.isSuccess()) {
            return new FirefoxAccount(result.consumeSuccess());
        } else {
            // TODO: Don't return a null thing
            return new FirefoxAccount(null);
        }
    }

    public String beginOAuthFlow(String redirectURI, String[] scopes, Boolean wantsKeys) {
        String scope = TextUtils.join(" ", scopes);
        RustResult result = JNA.INSTANCE.fxa_begin_oauth_flow(this.validPointer(), redirectURI, scope, wantsKeys);
        result.logIfFailure("FirefoxAccount.beginOAuthFlow");
        if (result.isSuccess()) {
            return result.consumeSuccess().getPointer(0).getString(0, "utf8");
        } else {
            // TODO: Don't return an empty string
            return "";
        }
    }
}

package io.github.mozilla.sandvich.rust;

import android.text.TextUtils;
import android.util.Log;

import com.sun.jna.Pointer;

public class FirefoxAccount extends RustObject<JNA.RawFxAccount> {

    public FirefoxAccount(JNA.RawFxAccount pointer) {
        this.rawPointer = pointer;
    }

    /* TODO: This throws a runtime exception if the fxa_new command doesn't complete for whatever
     *       reason (eg. no internet, no whatever). Ask Christian what to do with the super.
     */
    public FirefoxAccount(Config config, String clientId) {
        Error.ByReference e = new Error.ByReference();
        JNA.RawFxAccount result = JNA.INSTANCE.fxa_new(config.consumePointer(), clientId, e);
        if (e.isSuccess()) {
            this.rawPointer = result;
        } else {
            Log.e("FirefoxAccount.init", e.consumeMessage());
            this.rawPointer = null;
        }
    }

    @Override
    protected void destroyPointer(JNA.RawFxAccount fxa) {
        JNA.INSTANCE.fxa_free(fxa);
    }

    public static FirefoxAccount from(Config config, String clientId, String webChannelResponse) {
        Error.ByReference e = new Error.ByReference();
        JNA.RawFxAccount raw = JNA.INSTANCE.fxa_from_credentials(config.consumePointer(), clientId, webChannelResponse, e);
        if (e.isSuccess()) {
            return new FirefoxAccount(raw);
        } else {
            Log.e("fxa.from", e.consumeMessage());
            return null;
        }
    }


    public static FirefoxAccount fromJSONString(String json) {
        Error.ByReference e = new Error.ByReference();
        JNA.RawFxAccount raw = JNA.INSTANCE.fxa_from_json(json, e);
        if (e.isSuccess()) {
            return new FirefoxAccount(raw);
        } else {
            Log.e("fxa.fromJSONString", e.consumeMessage());
            return null;
        }
    }

    public String beginOAuthFlow(String redirectURI, String[] scopes, Boolean wantsKeys) {
        String scope = TextUtils.join(" ", scopes);
        Error.ByReference e = new Error.ByReference();
        Pointer p = JNA.INSTANCE.fxa_begin_oauth_flow(this.validPointer(), redirectURI, scope, wantsKeys, e);
        if (e.isSuccess()) {
            return getAndConsumeString(p);
        } else {
            Log.e("fxa.beginOAuthFlow", e.consumeMessage());
            return null;
        }
    }

    public Profile getProfile(boolean ignoreCache) {
        Error.ByReference e = new Error.ByReference();
        Profile.Raw p = JNA.INSTANCE.fxa_profile(this.validPointer(), ignoreCache, e);
        if (e.isSuccess()) {
            return new Profile(p);
        } else {
            Log.e("FirefoxAccount", e.consumeMessage());
            return null;
        }
    }

    public String newAssertion(String audience) {
        Error.ByReference e = new Error.ByReference();
        Pointer p = JNA.INSTANCE.fxa_assertion_new(this.validPointer(), audience, e);
        if (e.isSuccess()) {
            return getAndConsumeString(p);
        } else {
            Log.e("FirefoxAccount", e.consumeMessage());
            return null;
        }
    }

    public String getTokenServerEndpointURL() {
        Error.ByReference e = new Error.ByReference();
        Pointer p = JNA.INSTANCE.fxa_get_token_server_endpoint_url(this.validPointer(), e);
        if (e.isSuccess()) {
            return getAndConsumeString(p);
        } else {
            Log.e("FirefoxAccount", e.consumeMessage());
            return null;
        }
    }

    public SyncKeys getSyncKeys() {
        Error.ByReference e = new Error.ByReference();
        SyncKeys.Raw p = JNA.INSTANCE.fxa_get_sync_keys(this.validPointer(), e);
        if (e.isSuccess()) {
            return new SyncKeys(p);
        } else {
            Log.e("FirefoxAccount", e.consumeMessage());
            return null;
        }
    }

    public Profile getProfile() {
        return getProfile(false);
    }

    OAuthInfo completeOAuthFlow(String code, String state) {
        Error.ByReference e = new Error.ByReference();
        OAuthInfo.Raw p = JNA.INSTANCE.fxa_complete_oauth_flow(this.validPointer(), code, state, e);
        if (e.isSuccess()) {
            return new OAuthInfo(p);
        } else {
            Log.e("FirefoxAccount", e.consumeMessage());
            return null;
        }
    }

    OAuthInfo getOAuthToken(String scopes[]) {
        String scope = TextUtils.join(" ", scopes);
        Error.ByReference e = new Error.ByReference();
        OAuthInfo.Raw p = JNA.INSTANCE.fxa_get_oauth_token(this.validPointer(), scope, e);
        if (e.isSuccess()) {
            return new OAuthInfo(p);
        } else {
            Log.e("FirefoxAccount", e.consumeMessage());
            return null;
        }
    }

    public String completeOAuthFlow(String code, String state) {
        RustResult result = JNA.INSTANCE.fxa_complete_oauth_flow(this.rawPointer, code, state);
        if (result.isSuccess()) {
            Pointer ptr = result.ok;
            result.ok = null;
            return OAuthInfo(ptr);
        } else {
            Log.e("fxa.completeOAuthFlow", result.getError().message);
            return null;
        }
    }
}

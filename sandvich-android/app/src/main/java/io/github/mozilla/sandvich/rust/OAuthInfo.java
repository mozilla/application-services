package io.github.mozilla.sandvich.rust;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;

import java.util.Arrays;
import java.util.List;

public class OAuthInfo {
    /* package-local */
    static class Raw extends Structure {
        String access_token;
        String keys;
        String scope;

        Raw(Pointer p) {
            super(p);
            read();
        }

        @Override
        protected List<String> getFieldOrder() {
            return Arrays.asList("access_token", "keys", "scope");
        }
    }

    public final String accessToken;
    public final String keys;
    public final String scope;

    OAuthInfo(Raw raw) {
        this.accessToken = raw.access_token;
        this.keys = raw.keys;
        this.scope = raw.scope;
        JNA.INSTANCE.fxa_oauth_info_free(raw.getPointer());
    }

}

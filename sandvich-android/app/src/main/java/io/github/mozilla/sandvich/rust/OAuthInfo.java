package io.github.mozilla.sandvich.rust;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;

import java.util.Arrays;
import java.util.List;

public class OAuthInfo {
    /**
     * Represents a raw OAuthInfo pointer to a Rust struct.
     * Public for use with JNA; Raw should not be used in code beyond the FxA package.
     */
    /* package-local */
    public static class Raw extends Structure {
        public String access_token;
        public String keys;
        public String scope;

        public Raw(Pointer p) {
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

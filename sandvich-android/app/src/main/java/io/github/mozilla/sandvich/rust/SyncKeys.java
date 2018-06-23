package io.github.mozilla.sandvich.rust;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;

import java.util.Arrays;
import java.util.List;

public class SyncKeys {
    /* package-local */
    static class Raw extends Structure {
        String sync_key;
        String xcs;

        Raw(Pointer p) {
            super(p);
            read();
        }
        @Override
        protected List<String> getFieldOrder() {
            return Arrays.asList("sync_key", "xcs");
        }
    }

    public final String syncKey;
    public final String xcs;

    SyncKeys(Raw raw) {
        this.syncKey = raw.sync_key;
        this.xcs = raw.xcs;
        JNA.INSTANCE.fxa_sync_keys_free(raw.getPointer());
    }

}

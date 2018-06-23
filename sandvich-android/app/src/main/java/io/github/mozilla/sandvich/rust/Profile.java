package io.github.mozilla.sandvich.rust;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;

import java.util.Arrays;
import java.util.List;

public class Profile {
    /* package-local */
    static class Raw extends Structure {
        String uid;
        String email;
        String avatar;
        String display_name;

        Raw(Pointer p) {
            super(p);
            read();
        }
        @Override
        protected List<String> getFieldOrder() {
            return Arrays.asList("uid", "email", "avatar", "display_name");
        }
    }

    public final String uid;
    public final String email;
    public final String avatar;
    public final String displayName;

    Profile(Profile.Raw raw) {
        this.uid = raw.uid;
        this.email = raw.email;
        this.avatar = raw.avatar;
        this.displayName = raw.display_name;
        JNA.INSTANCE.fxa_profile_free(raw.getPointer());
    }
}

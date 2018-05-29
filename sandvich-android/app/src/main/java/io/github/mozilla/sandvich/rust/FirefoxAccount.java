package io.github.mozilla.sandvich.rust;

import com.sun.jna.Pointer;

public class FirefoxAccount extends RustObject {

    public FirefoxAccount(Pointer pointer) {
        this.rawPointer = pointer;
    }

    @Override
    public void close() {
        if (this.rawPointer != null) {
            //JNA.INSTANCE.tx_report_destroy(this.rawPointer);
        }
    }
}

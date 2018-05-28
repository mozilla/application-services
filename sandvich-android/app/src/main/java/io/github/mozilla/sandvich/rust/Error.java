package io.github.mozilla.sandvich.rust;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;

import java.util.Arrays;
import java.util.List;

public class Error extends Structure {

    public Error (Pointer ptr) {
        super (ptr);
    }

//
//    public static interface ErrorCode {
//        public static final int Other = 0;
//        public static final int AuthenticationError = 1;
//    }
//
//    public enum ErrorCode {
//        Other, AuthenticationError
//    }

    public int code;
    public String message;

    @Override
    protected List<String> getFieldOrder() {
        return Arrays.asList("code", "message");
    }
}

package io.github.mozilla.sandvich.rust;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.NativeLibrary;
import com.sun.jna.Pointer;

public interface JNA extends Library {
    String JNA_LIBRARY_NAME = "fxa_client";
    NativeLibrary JNA_NATIVE_LIB = NativeLibrary.getInstance(JNA_LIBRARY_NAME);

    JNA INSTANCE = (JNA) Native.loadLibrary(JNA_LIBRARY_NAME, JNA.class);

    RustResult fxa_get_release_config();
    RustResult fxa_get_custom_config(String content_base);
//    let config = FxAConfig.custom(content_base: "https://sandvich-ios.dev.lcip.org");
//            fxa = FirefoxAccount(config: config, clientId: "22d74070a481bc73")

    RustResult fxa_new(Pointer config, String clientId);
    RustResult fxa_from_credentials(Pointer config, String clientId, String webChannelResponse);
    RustResult fxa_begin_oauth_flow(Pointer fxa, String redirectUri, String scopes, boolean wantsKeys);
    RustResult fxa_complete_oauth_flow(Pointer fxa, String code, String state);
    void fxa_config_free(Pointer config);
    void fxa_str_free(Pointer string);
    void fxa_free(Pointer fxa);
}
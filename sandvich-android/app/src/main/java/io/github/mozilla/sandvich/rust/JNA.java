package io.github.mozilla.sandvich.rust;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.NativeLibrary;
import com.sun.jna.Pointer;
import com.sun.jna.PointerType;

public interface JNA extends Library {
    String JNA_LIBRARY_NAME = "fxa_client";
    NativeLibrary JNA_NATIVE_LIB = NativeLibrary.getInstance(JNA_LIBRARY_NAME);

    JNA INSTANCE = (JNA) Native.loadLibrary(JNA_LIBRARY_NAME, JNA.class);

    class RawFxAccount extends PointerType {}
    class RawConfig extends PointerType {}

    RawConfig fxa_get_release_config(Error.ByReference e);
    RawConfig fxa_get_custom_config(String content_base, Error.ByReference e);

    RawFxAccount fxa_new(RawConfig config, String clientId, Error.ByReference e);
    RawFxAccount fxa_from_credentials(RawConfig config, String clientId, String webChannelResponse, Error.ByReference e);

    RawFxAccount fxa_from_json(String json, Error.ByReference e);

    Pointer fxa_begin_oauth_flow(RawFxAccount fxa, String redirectUri, String scopes, boolean wantsKeys, Error.ByReference e); // string pointer
    Profile.Raw fxa_profile(RawFxAccount fxa, boolean ignoreCache, Error.ByReference e);
    Pointer fxa_assertion_new(RawFxAccount fxa, String audience, Error.ByReference e); // string pointer
    Pointer fxa_get_token_server_endpoint_url(RawFxAccount fxa, Error.ByReference e); // string pointer

    SyncKeys.Raw fxa_get_sync_keys(RawFxAccount fxa, Error.ByReference e);

    OAuthInfo.Raw fxa_complete_oauth_flow(RawFxAccount fxa, String code, String state, Error.ByReference e);
    OAuthInfo.Raw fxa_get_oauth_token(RawFxAccount fxa, String scope, Error.ByReference e);

    void fxa_config_free(RawConfig config);
    void fxa_str_free(Pointer string);
    void fxa_free(RawFxAccount fxa);

    // In theory these would take `OAuthInfo.Raw.ByReference` (and etc), but
    // the rust functions that return these return `OAuthInfo.Raw` and not
    // the ByReference subtypes. So I'm not sure there's a way to do this
    // when using Structure.
    void fxa_oauth_info_free(Pointer ptr);
    void fxa_profile_free(Pointer ptr);
    void fxa_sync_keys_free(Pointer ptr);
}

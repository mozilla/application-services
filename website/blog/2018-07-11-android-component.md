---
title: Firefox Accounts Android Component
author: Vlad Filippov
authorURL: https://github.com/vladikoff
---

The initial version of the Firefox Accounts Android component has been released as part of [android-components 0.12](https://github.com/mozilla-mobile/android-components/releases) several weeks ago.


<!--truncate-->

```kt
companion object {
    private const val JNA_LIBRARY_NAME = "fxa_client"
    private val JNA_NATIVE_LIB: Any
    internal val INSTANCE: FxaClient

    init {
        System.loadLibrary(JNA_LIBRARY_NAME)
        JNA_NATIVE_LIB = NativeLibrary.getInstance(JNA_LIBRARY_NAME)
        INSTANCE = Native.loadLibrary(JNA_LIBRARY_NAME, FxaClient::class.java) as FxaClient
    }
}
```

This new component consumes the new [fxa-rust-client](https://github.com/mozilla/application-services/tree/master/fxa-rust-client), which allows us to write things once and later cross-compile the code to different mobile platforms. As part of developing this component we utilize the following technologies: Rust, Kotlin, JNA, JOSE and more.

Since the initial release the [team](https://github.com/mozilla-mobile/android-components/graphs/contributors) already made various improvements, such as making method calls asynchronous, improving error handling and slimming down the size of the library. The component is available as a “tech preview” and you can try it in a [sample Android app](https://github.com/mozilla-mobile/android-components/tree/master/samples/firefox-accounts).

![](/application-services/img/blog/2018-07-11/and-comp2.jpg)

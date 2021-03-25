
# Rust + Android FAQs

### How do I expose Rust code to Kotlin?

Use [UniFFI](https://mozilla.github.io/uniffi-rs/), which can produce Kotlin
bindings for your Rust code from an interface definition file.

If UniFFI doesn't currently meet your needs, please [open an issue](
https://github.com/mozilla/uniffi-rs/issues) to discuss how the tool can
be improved.

As a last resort, you can make hand-written bindings from Rust to Kotlin,
essentially manually performing the steps that UniFFI tries to automate
for you:

* Expose some `pub extern "C"` functions from your Rust code.  Use the
  [ffi_support](https://docs.rs/ffi-support/0.1.3/ffi_support/) crate to help make
  this easier, and consult the crate's documentation for tips and gotchas.
* Write some low-level Kotlin bindings for the exposed functions, using
  [JNA](https://github.com/java-native-access/jna) to load the compiled rust
  code via shared library. Our [android support package](../components/support/android)
  has some helpers to make this easier.
* Write some higher-level Kotlin code that wraps those bindings in a nice safe
  ergonomic Kotlin API, trying to mirror the structure of the original Rust code
  as much as possible.

### How should I name the package?

Published packages should be named `org.mozilla.appservices.$NAME` where `$NAME`
is the name of your component, such as `logins`.  The Java namespace in which
your package defines its classes etc should be `mozilla.appservices.$NAME.*`.

### How do I publish the resulting package?

Add it to `.buildconfig-android.yml` in the root of this repository.
This will cause it to be automatically included as part of our release
publishing pipeline.

### How do I know what library name to load to access the compiled rust code?

Assuming that you're building the Rust code as part of the application-services
build and release process, your `pub extern "C"` API should always be available
from a file named `libmegazord.so`.

### What challenges exist when calling back into Kotlin from Rust?

There are a number of them. The issue boils down to the fact that you need to be
completely certain that a JVM is associated with a given thread in order to call
java code on it. The difficulty is that the JVM can GC its threads and will not
let rust know about it. JNA can work around this for us to some extent, however
there are difficulties.

The approach it takes is essentially to spawn a thread for each callback
invocation. If you are certain you’re going to do a lot of callbacks and they
all originate on the same thread, you can tell it to cache these.

Calling back from Rust into Kotlin isn’t too bad so long as you ensure the
callback can not be GCed while rust code holds onto it, and you can either
accept the overhead of extra threads being instantiated on each call, or you can
ensure that it only happens from a single thread.

Note that the situation would be somewhat better if we used JNI directly (and
not JNA), but this would cause us to need to write two versions of each ffi
crate, one for iOS, and one for Android.

Ultimately, in any case where you can reasonably move to making something a
blocking call, do so. It’s very easy to run such things on a background thread
in Kotlin. This is in line with the Android documentation on JNI usage, and my
own experience. It’s vastly simpler and less painful this way.

(Of course, not every case is solvable like this).

### Why are we using JNA rather than JNI, and what tradeoffs does that involve?

We get a couple things from using JNA that we wouldn't with JNI.

1. We are able to write a *single* FFI crate. If we used JNI we'd need to write
   one FFI that android calls, and one that iOS calls.

2. JNA provides a mapping of threads to callbacks for us, making callbacks over
   the FFI possible. That said, in practice this is still error prone, and easy
   to misuse/cause memory safety bugs, but it's required for cases like logging,
   among others, and so it is a nontrivial piece of complexity we'd have to
   reimplement.

However, it comes with the following downsides:

1. JNA has bugs. In particular, its not safe to use bools with them, it thinks
   they are 32 bits, when on most platforms (every platform Rust supports) they
   are 8 bits. They've been unwilling to fix the issue due to it breaking
   backwards compatibility (which is... somewhat fair, there is a lot of C89
   code out there that uses `bool` as a typedef for a 32-bit `int`).
2. JNA makes it really easy to do the wrong thing and have it work but corrupt
   memory. Several of the caveats around this are documented in the
   [`ffi_support` docs](https://docs.rs/ffi-support/*/ffi_support/), but a
   major one is when to use `Pointer` vs `String` (getting this wrong will
   often work, but may corrupt memory).

### How do I debug Rust code with the step-debugger in Android Studio

1. Uncomment the `packagingOptions { doNotStrip "**/*.so" }` line from the
   build.gradle file of the component you want to debug.
2. In the rust code, either:
    1. Cause something to crash where you want the breakpoint. Note: Panics
        don't work here, unfortunately. (I have not found a convenient way to
        set a breakpoint to rust code, so
        `unsafe { std::ptr::write_volatile(0 as *const _, 1u8) }` usually is
        what I do).
    2. If you manage to get an LLDB prompt, you can set a breakpoint using
       `breakpoint set --name foo`, or `breakpoint set --file foo.rs --line 123`.
       I don't know how to bring up this prompt reliably, so I often do step 1 to
       get it to appear, delete the crashing code, and then set the
       breakpoint using the CLI. This is admittedly suboptimal.
3. Click the Debug button in Android Studio, to display the "Select Deployment
   Target" window.
4. Make sure the debugger selection is set to "Both". This tends to unset
   itself, so make sure.
5. Click "Run", and debug away.

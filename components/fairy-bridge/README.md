# Fairy Bridge

Fairy Bridge is an HTTP request bridge library that allows requests to be made using various
backends, including:

  - The builtin reqwest backend
  - Custom Rust backends
  - Custom backends written in the foreign language

The plan for this is:
  - iOS will use the reqwest backend
  - Android will use a custom backend in Kotlin using fetch
    (https://github.com/mozilla-mobile/firefox-android/tree/35ce01367157440f9e9daa4ed48a8022af80c8f2/android-components/components/concept/fetch)
  - Desktop will use a custom backend in Rust that hooks into necko

## Sync / Async

The backends are implemented using async code, but there's also the option to block on a request.
This means `fairy-bridge` can be used in both sync and async contexts.

## Cookies / State

Cookies and state are outside the scope of this library. Any such functionality is the responsibility of the consumer.

## Name

`fairy-bridge` is named after the Fairy Bridge (Xian Ren Qiao) -- the largest known natural bridge in the world, located in northwestern Guangxi Province, China.

![Picture of the Fairy Bridge](http://www.naturalarches.org/big9_files/FairyBridge1680.jpg)

# Backends

## Reqwest

- Handle requests using the Rust [reqwest library](https://docs.rs/reqwest/latest/reqwest/).
- This backend creates a tokio thread to execute the requests.
- Call `fairy_bridge::init_backend_reqwest` to select this backend.

## Foreign code

- The foreign code can implement a backend themselves by implementing the `fairy_bridge::Backend` trait.
- Pass an instance of the object that implements the trait to  `fairy_bridge::init_backend` to select this backend.

## C / C++ code

- A backend can also be implemented in C / C++ code
- Include the `c-backend-include/fairy_bridge.h` file.
- Implement the `fairy_bridge_backend_c_send_request` function.
- Call `fairy_bridge::init_backend_c` to select this backend (from the bindings language, not C).
- See `examples/fairy-bridge-demo` for a code example.

## (Coming soon) Necko backend

- The geckoview `libxul` library comes with a Necko-based c backend.
- Link to `libxul` and call `fairy_bridge::init_backend_c` to select this backend.

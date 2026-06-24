# Viaduct

Viaduct is our HTTP request library, which allows components to make HTTP request.
Normally this means bridging to the application HTTP library.

How the request is handled depends on the viaduct backend that the application sets up.
This can either be a Rust crate or foreign code that implements the `Backend` interface.
Here's how it works for different application:

| Application | Backend | Notes |
|-------------|---------|-------|
| Desktop | `viaduct-necko` | Lives in the moz-central repo and forwards requests to the `necko` libary |
| Android | Kotlin-implemented | Also handled by `necko`, but there's a longer chain of bridge code. Kotlin code implements a backend by forwarding requests to the `fetch` library, which then forwards to `GeckoView` and the end result is that `necko` handles the request.|
| iOS | `viaduct-hyper` | Forwards requests to the Rust `hyper` library.  Creates and manages a thread to process the requests |
| testing | `viaduct-dev` | Forwards requests to `minireq` |

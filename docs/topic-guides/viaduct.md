# Viaduct

[`Viaduct`](https://github.com/mo zilla/application-services/blob/main/components/viaduct/README.md) initialization is required for all platforms and for multiple components.

There are 3 different options to use `viaduct`:

* Any `libxul` based can ignore initialization, since it's handled by `libxul`.
* Using the reqwest backend, which uses the `reqwest` library and a `reqwest`-managed thread.
* Implementing the C FFI like `libxul` does (https://searchfox.org/mozilla-central/source/toolkit/components/viaduct).
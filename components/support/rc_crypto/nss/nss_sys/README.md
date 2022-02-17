## nss_sys

Low-level NSS bindings for Rust.

This crate defines low-level FFI bindings for NSS. They are maintained by hand,
and their correctness is verified using the `systest` crate in the parent directory.

The directory structure of this crate is meant to mirror that of NSS itself.
For each header file provided by NSS, there should be a corresponding `.rs` file
in the `nss_sys::bindings` module that declares the corresponding functions and
data types.

To add new bindings in this crate, you'll need to:

* Identify the NSS header file that contains the functionality of interest.
* Edit the Rust file of the corresponding name under `./src/bindings`.
    * If one doesn't currently exist then create it, and also add the corresponding
      header to the list of headers in `../systest/build.rs`.
* Add `#[recpr(C)]` structs and `pub extern "C"` functions as necessary to make the
  new functionality visible to Rust.
* Verify the correctness of your additions using `cargo run -p systest` in the repository root.

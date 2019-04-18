# rc_crypto

rc_crypto, like its name infers, handles all of our cryptographic needs.

It is backed by the Mozilla-sponsored NSS library through the `nss-sys` crate (more information [here](nss_sys/README.md)).

It pretty much follows the very rust-idiomatic [ring crate API](https://briansmith.org/rustdoc/ring/).

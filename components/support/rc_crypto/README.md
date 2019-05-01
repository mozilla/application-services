# rc_crypto

rc_crypto, like its name infers, handles all of our cryptographic needs.

It is backed by the Mozilla-sponsored NSS library through the `nss-sys` crate (more information [here](nss_sys/README.md)).

It pretty much follows the very rust-idiomatic [ring crate API](https://briansmith.org/rustdoc/ring/).

## License

This derives its API and portions of its implementation from the the [`ring`](https://github.com/briansmith/ring/) project, which is available under an ISC-style license. See the COPYRIGHT file, or https://github.com/briansmith/ring/blob/master/LICENSE for details.

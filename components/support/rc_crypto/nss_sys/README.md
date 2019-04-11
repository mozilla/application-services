## nss_sys

NSS bindings for Rust.

These bindings are implemented using `dlopen`/`dlsym` instead of linking against libnss.
This is so we can re-use the NSS library shipped with GeckoView on Fenix and reference-browser.
On Lockbox Android, or even in unit tests artifacts, we ship these library files ourselves alongside our compiled Rust library.

On iOS the situation is different, we dynamically link because Apple [discourages using `dlopen`](https://github.com/nicklockwood/GZIP/issues/24).

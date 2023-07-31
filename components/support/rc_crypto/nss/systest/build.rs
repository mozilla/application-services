/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{env, ffi::OsString, path::Path};

fn main() {
    let mut cfg = ctest2::TestGenerator::new();
    cfg.header("blapit.h")
        .header("cert.h")
        .header("certt.h")
        .header("certdb.h")
        .header("keyhi.h")
        .header("keythi.h")
        .header("stdbool.h")
        .header("mozpkix/pkixc.h")
        .header("nss.h")
        .header("pk11pub.h")
        .header("pkcs11n.h")
        .header("pkcs11t.h")
        .header("plarena.h")
        .header("prerror.h")
        .header("prtypes.h")
        .header("secasn1t.h")
        .header("seccomon.h")
        .header("secitem.h")
        .header("secmodt.h")
        .header("secoid.h")
        .header("secoidt.h")
        .header("secport.h");

    println!("cargo:rerun-if-env-changed=NSS_DIR");
    let nss_dir: OsString = env::var_os("NSS_DIR").unwrap();
    let nss_dir = Path::new(&nss_dir);
    let include_dir = nss_dir.join("include").join("nss");

    // Include the directory where the header files are defined
    cfg.include(include_dir);

    cfg.field_name(|_s, field| field.replace("type_", "type"));

    cfg.skip_type(|s| {
        // Opaque types.
        s == "CERTCertDBHandle"
            || s == "CERTCertificate"
            || s == "PK11SlotInfo"
            || s == "PK11SymKey"
            || s == "PK11Context"
            || s == "NSSInitContext"
            || s == "NSSInitParameters"
            || s == "PK11GenericObject"
    });
    cfg.skip_field_type(|s, field| {
        s == "SECKEYPublicKeyStr" && field == "u" // inline union
    });
    cfg.skip_struct(|s| {
        s == "SECKEYPublicKeyStr_u" // inline union
    });

    // Obscure test failures only under WSL (#4165) so skip it.
    cfg.skip_fn(|s| s == "PK11_CreateContextBySymKey");

    // Generate the tests, passing the path to the `*-sys` library as well as
    // the module to generate.
    cfg.generate("../nss_sys/src/lib.rs", "all.rs");
}

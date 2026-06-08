# Licenses for Third-Party Dependencies

Binary distributions of this software incorporate code from a number of third-party dependencies.
These dependencies are available under a variety of free and open source licenses,
the details of which are reproduced below.

* [Mozilla Public License 2.0](#mozilla-public-license-20)
* [Apache License 2.0](#apache-license-20)
* [MIT License: aho-corasick, byteorder, memchr](#mit-license-aho-corasick-byteorder-memchr)
* [MIT License: bincode](#mit-license-bincode)
* [MIT License: bytes](#mit-license-bytes)
* [MIT License: cargo_metadata, winnow](#mit-license-cargo_metadata-winnow)
* [MIT License: core_maths](#mit-license-core_maths)
* [MIT License: generic-array](#mit-license-generic-array)
* [MIT License: goblin](#mit-license-goblin)
* [MIT License: h2](#mit-license-h2)
* [MIT License: http-body](#mit-license-http-body)
* [MIT License: hyper](#mit-license-hyper)
* [MIT License: libsqlite3-sys, rusqlite](#mit-license-libsqlite3-sys-rusqlite)
* [MIT License: mio](#mit-license-mio)
* [MIT License: nom](#mit-license-nom)
* [MIT License: ordered-float](#mit-license-ordered-float)
* [MIT License: scroll](#mit-license-scroll)
* [MIT License: scroll_derive](#mit-license-scroll_derive)
* [MIT License: sharded-slab](#mit-license-sharded-slab)
* [MIT License: slab](#mit-license-slab)
* [MIT License: smawk](#mit-license-smawk)
* [MIT License: synstructure](#mit-license-synstructure)
* [MIT License: textwrap](#mit-license-textwrap)
* [MIT License: tokio](#mit-license-tokio)
* [MIT License: tokio-native-tls, tracing, tracing-attributes, tracing-core, tracing-subscriber](#mit-license-tokio-native-tls-tracing-tracing-attributes-tracing-core-tracing-subscriber)
* [MIT License: tokio-util](#mit-license-tokio-util)
* [MIT License: tower-service](#mit-license-tower-service)
* [MIT License: try-lock](#mit-license-try-lock)
* [MIT License: want](#mit-license-want)
* [MIT License: weedle2](#mit-license-weedle2)
* [ISC License: libloading](#isc-license-libloading)
* [BSD-2-Clause License: arrayref](#bsd-2-clause-license-arrayref)
* [BSD-3-Clause License: bindgen](#bsd-3-clause-license-bindgen)
* [Zlib License: foldhash](#zlib-license-foldhash)
* [Unicode-3.0 License: icu_collections, icu_locale, icu_locale_core, icu_locale_data, icu_normalizer, icu_normalizer_data, icu_properties, icu_properties_data, icu_provider, icu_segmenter, icu_segmenter_data, litemap, potential_utf, tinystr, writeable, yoke, yoke-derive, zerofrom, zerofrom-derive, zerotrie, zerovec, zerovec-derive](#unicode-30-license-icu_collections-icu_locale-icu_locale_core-icu_locale_data-icu_normalizer-icu_normalizer_data-icu_properties-icu_properties_data-icu_provider-icu_segmenter-icu_segmenter_data-litemap-potential_utf-tinystr-writeable-yoke-yoke-derive-zerofrom-zerofrom-derive-zerotrie-zerovec-zerovec-derive)
* [Optional Notice: SQLite](#optional-notice-sqlite)
* [(MIT OR Apache-2.0) AND Unicode-3.0 License: unicode-ident](#(mit-or-apache-20)-and-unicode-30-license-unicode-ident)
-------------
## Mozilla Public License 2.0

The following text applies to code linked from these dependencies:
[jexl-eval](https://github.com/mozilla/jexl-rs),
[jexl-parser](https://github.com/mozilla/jexl-rs),
[uniffi](https://github.com/mozilla/uniffi-rs),
[uniffi_bindgen](https://github.com/mozilla/uniffi-rs),
[uniffi_build](https://github.com/mozilla/uniffi-rs),
[uniffi_core](https://github.com/mozilla/uniffi-rs),
[uniffi_internal_macros](https://github.com/mozilla/uniffi-rs),
[uniffi_macros](https://github.com/mozilla/uniffi-rs),
[uniffi_meta](https://github.com/mozilla/uniffi-rs),
[uniffi_pipeline](https://github.com/mozilla/uniffi-rs),
[uniffi_udl](https://github.com/mozilla/uniffi-rs)

```
Mozilla Public License Version 2.0
==================================

1. Definitions
--------------

1.1. "Contributor"
    means each individual or legal entity that creates, contributes to
    the creation of, or owns Covered Software.

1.2. "Contributor Version"
    means the combination of the Contributions of others (if any) used
    by a Contributor and that particular Contributor's Contribution.

1.3. "Contribution"
    means Covered Software of a particular Contributor.

1.4. "Covered Software"
    means Source Code Form to which the initial Contributor has attached
    the notice in Exhibit A, the Executable Form of such Source Code
    Form, and Modifications of such Source Code Form, in each case
    including portions thereof.

1.5. "Incompatible With Secondary Licenses"
    means

    (a) that the initial Contributor has attached the notice described
        in Exhibit B to the Covered Software; or

    (b) that the Covered Software was made available under the terms of
        version 1.1 or earlier of the License, but not also under the
        terms of a Secondary License.

1.6. "Executable Form"
    means any form of the work other than Source Code Form.

1.7. "Larger Work"
    means a work that combines Covered Software with other material, in
    a separate file or files, that is not Covered Software.

1.8. "License"
    means this document.

1.9. "Licensable"
    means having the right to grant, to the maximum extent possible,
    whether at the time of the initial grant or subsequently, any and
    all of the rights conveyed by this License.

1.10. "Modifications"
    means any of the following:

    (a) any file in Source Code Form that results from an addition to,
        deletion from, or modification of the contents of Covered
        Software; or

    (b) any new file in Source Code Form that contains any Covered
        Software.

1.11. "Patent Claims" of a Contributor
    means any patent claim(s), including without limitation, method,
    process, and apparatus claims, in any patent Licensable by such
    Contributor that would be infringed, but for the grant of the
    License, by the making, using, selling, offering for sale, having
    made, import, or transfer of either its Contributions or its
    Contributor Version.

1.12. "Secondary License"
    means either the GNU General Public License, Version 2.0, the GNU
    Lesser General Public License, Version 2.1, the GNU Affero General
    Public License, Version 3.0, or any later versions of those
    licenses.

1.13. "Source Code Form"
    means the form of the work preferred for making modifications.

1.14. "You" (or "Your")
    means an individual or a legal entity exercising rights under this
    License. For legal entities, "You" includes any entity that
    controls, is controlled by, or is under common control with You. For
    purposes of this definition, "control" means (a) the power, direct
    or indirect, to cause the direction or management of such entity,
    whether by contract or otherwise, or (b) ownership of more than
    fifty percent (50%) of the outstanding shares or beneficial
    ownership of such entity.

2. License Grants and Conditions
--------------------------------

2.1. Grants

Each Contributor hereby grants You a world-wide, royalty-free,
non-exclusive license:

(a) under intellectual property rights (other than patent or trademark)
    Licensable by such Contributor to use, reproduce, make available,
    modify, display, perform, distribute, and otherwise exploit its
    Contributions, either on an unmodified basis, with Modifications, or
    as part of a Larger Work; and

(b) under Patent Claims of such Contributor to make, use, sell, offer
    for sale, have made, import, and otherwise transfer either its
    Contributions or its Contributor Version.

2.2. Effective Date

The licenses granted in Section 2.1 with respect to any Contribution
become effective for each Contribution on the date the Contributor first
distributes such Contribution.

2.3. Limitations on Grant Scope

The licenses granted in this Section 2 are the only rights granted under
this License. No additional rights or licenses will be implied from the
distribution or licensing of Covered Software under this License.
Notwithstanding Section 2.1(b) above, no patent license is granted by a
Contributor:

(a) for any code that a Contributor has removed from Covered Software;
    or

(b) for infringements caused by: (i) Your and any other third party's
    modifications of Covered Software, or (ii) the combination of its
    Contributions with other software (except as part of its Contributor
    Version); or

(c) under Patent Claims infringed by Covered Software in the absence of
    its Contributions.

This License does not grant any rights in the trademarks, service marks,
or logos of any Contributor (except as may be necessary to comply with
the notice requirements in Section 3.4).

2.4. Subsequent Licenses

No Contributor makes additional grants as a result of Your choice to
distribute the Covered Software under a subsequent version of this
License (see Section 10.2) or under the terms of a Secondary License (if
permitted under the terms of Section 3.3).

2.5. Representation

Each Contributor represents that the Contributor believes its
Contributions are its original creation(s) or it has sufficient rights
to grant the rights to its Contributions conveyed by this License.

2.6. Fair Use

This License is not intended to limit any rights You have under
applicable copyright doctrines of fair use, fair dealing, or other
equivalents.

2.7. Conditions

Sections 3.1, 3.2, 3.3, and 3.4 are conditions of the licenses granted
in Section 2.1.

3. Responsibilities
-------------------

3.1. Distribution of Source Form

All distribution of Covered Software in Source Code Form, including any
Modifications that You create or to which You contribute, must be under
the terms of this License. You must inform recipients that the Source
Code Form of the Covered Software is governed by the terms of this
License, and how they can obtain a copy of this License. You may not
attempt to alter or restrict the recipients' rights in the Source Code
Form.

3.2. Distribution of Executable Form

If You distribute Covered Software in Executable Form then:

(a) such Covered Software must also be made available in Source Code
    Form, as described in Section 3.1, and You must inform recipients of
    the Executable Form how they can obtain a copy of such Source Code
    Form by reasonable means in a timely manner, at a charge no more
    than the cost of distribution to the recipient; and

(b) You may distribute such Executable Form under the terms of this
    License, or sublicense it under different terms, provided that the
    license for the Executable Form does not attempt to limit or alter
    the recipients' rights in the Source Code Form under this License.

3.3. Distribution of a Larger Work

You may create and distribute a Larger Work under terms of Your choice,
provided that You also comply with the requirements of this License for
the Covered Software. If the Larger Work is a combination of Covered
Software with a work governed by one or more Secondary Licenses, and the
Covered Software is not Incompatible With Secondary Licenses, this
License permits You to additionally distribute such Covered Software
under the terms of such Secondary License(s), so that the recipient of
the Larger Work may, at their option, further distribute the Covered
Software under the terms of either this License or such Secondary
License(s).

3.4. Notices

You may not remove or alter the substance of any license notices
(including copyright notices, patent notices, disclaimers of warranty,
or limitations of liability) contained within the Source Code Form of
the Covered Software, except that You may alter any license notices to
the extent required to remedy known factual inaccuracies.

3.5. Application of Additional Terms

You may choose to offer, and to charge a fee for, warranty, support,
indemnity or liability obligations to one or more recipients of Covered
Software. However, You may do so only on Your own behalf, and not on
behalf of any Contributor. You must make it absolutely clear that any
such warranty, support, indemnity, or liability obligation is offered by
You alone, and You hereby agree to indemnify every Contributor for any
liability incurred by such Contributor as a result of warranty, support,
indemnity or liability terms You offer. You may include additional
disclaimers of warranty and limitations of liability specific to any
jurisdiction.

4. Inability to Comply Due to Statute or Regulation
---------------------------------------------------

If it is impossible for You to comply with any of the terms of this
License with respect to some or all of the Covered Software due to
statute, judicial order, or regulation then You must: (a) comply with
the terms of this License to the maximum extent possible; and (b)
describe the limitations and the code they affect. Such description must
be placed in a text file included with all distributions of the Covered
Software under this License. Except to the extent prohibited by statute
or regulation, such description must be sufficiently detailed for a
recipient of ordinary skill to be able to understand it.

5. Termination
--------------

5.1. The rights granted under this License will terminate automatically
if You fail to comply with any of its terms. However, if You become
compliant, then the rights granted under this License from a particular
Contributor are reinstated (a) provisionally, unless and until such
Contributor explicitly and finally terminates Your grants, and (b) on an
ongoing basis, if such Contributor fails to notify You of the
non-compliance by some reasonable means prior to 60 days after You have
come back into compliance. Moreover, Your grants from a particular
Contributor are reinstated on an ongoing basis if such Contributor
notifies You of the non-compliance by some reasonable means, this is the
first time You have received notice of non-compliance with this License
from such Contributor, and You become compliant prior to 30 days after
Your receipt of the notice.

5.2. If You initiate litigation against any entity by asserting a patent
infringement claim (excluding declaratory judgment actions,
counter-claims, and cross-claims) alleging that a Contributor Version
directly or indirectly infringes any patent, then the rights granted to
You by any and all Contributors for the Covered Software under Section
2.1 of this License shall terminate.

5.3. In the event of termination under Sections 5.1 or 5.2 above, all
end user license agreements (excluding distributors and resellers) which
have been validly granted by You or Your distributors under this License
prior to termination shall survive termination.

************************************************************************
*                                                                      *
*  6. Disclaimer of Warranty                                           *
*  -------------------------                                           *
*                                                                      *
*  Covered Software is provided under this License on an "as is"       *
*  basis, without warranty of any kind, either expressed, implied, or  *
*  statutory, including, without limitation, warranties that the       *
*  Covered Software is free of defects, merchantable, fit for a        *
*  particular purpose or non-infringing. The entire risk as to the     *
*  quality and performance of the Covered Software is with You.        *
*  Should any Covered Software prove defective in any respect, You     *
*  (not any Contributor) assume the cost of any necessary servicing,   *
*  repair, or correction. This disclaimer of warranty constitutes an   *
*  essential part of this License. No use of any Covered Software is   *
*  authorized under this License except under this disclaimer.         *
*                                                                      *
************************************************************************

************************************************************************
*                                                                      *
*  7. Limitation of Liability                                          *
*  --------------------------                                          *
*                                                                      *
*  Under no circumstances and under no legal theory, whether tort      *
*  (including negligence), contract, or otherwise, shall any           *
*  Contributor, or anyone who distributes Covered Software as          *
*  permitted above, be liable to You for any direct, indirect,         *
*  special, incidental, or consequential damages of any character      *
*  including, without limitation, damages for lost profits, loss of    *
*  goodwill, work stoppage, computer failure or malfunction, or any    *
*  and all other commercial damages or losses, even if such party      *
*  shall have been informed of the possibility of such damages. This   *
*  limitation of liability shall not apply to liability for death or   *
*  personal injury resulting from such party's negligence to the       *
*  extent applicable law prohibits such limitation. Some               *
*  jurisdictions do not allow the exclusion or limitation of           *
*  incidental or consequential damages, so this exclusion and          *
*  limitation may not apply to You.                                    *
*                                                                      *
************************************************************************

8. Litigation
-------------

Any litigation relating to this License may be brought only in the
courts of a jurisdiction where the defendant maintains its principal
place of business and such litigation shall be governed by laws of that
jurisdiction, without reference to its conflict-of-law provisions.
Nothing in this Section shall prevent a party's ability to bring
cross-claims or counter-claims.

9. Miscellaneous
----------------

This License represents the complete agreement concerning the subject
matter hereof. If any provision of this License is held to be
unenforceable, such provision shall be reformed only to the extent
necessary to make it enforceable. Any law or regulation which provides
that the language of a contract shall be construed against the drafter
shall not be used to construe this License against a Contributor.

10. Versions of the License
---------------------------

10.1. New Versions

Mozilla Foundation is the license steward. Except as provided in Section
10.3, no one other than the license steward has the right to modify or
publish new versions of this License. Each version will be given a
distinguishing version number.

10.2. Effect of New Versions

You may distribute the Covered Software under the terms of the version
of the License under which You originally received the Covered Software,
or under the terms of any subsequent version published by the license
steward.

10.3. Modified Versions

If you create software not governed by this License, and you want to
create a new license for such software, you may create and use a
modified version of this License if you rename the license and remove
any references to the name of the license steward (except to note that
such modified license differs from this License).

10.4. Distributing Source Code Form that is Incompatible With Secondary
Licenses

If You choose to distribute Source Code Form that is Incompatible With
Secondary Licenses under the terms of this version of the License, the
notice described in Exhibit B of this License must be attached.

Exhibit A - Source Code Form License Notice
-------------------------------------------

  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at http://mozilla.org/MPL/2.0/.

If it is not possible or desirable to put the notice in a particular
file, then You may include the notice in a location (such as a LICENSE
file in a relevant directory) where a recipient would be likely to look
for such a notice.

You may add additional accurate notices of copyright ownership.

Exhibit B - "Incompatible With Secondary Licenses" Notice
---------------------------------------------------------

  This Source Code Form is "Incompatible With Secondary Licenses", as
  defined by the Mozilla Public License, v. 2.0.

```
-------------
## Apache License 2.0

The following text applies to code linked from these dependencies:
[anyhow](https://github.com/dtolnay/anyhow),
[askama](https://github.com/askama-rs/askama),
[askama_derive](https://github.com/askama-rs/askama),
[askama_macros](https://github.com/askama-rs/askama),
[askama_parser](https://github.com/askama-rs/askama),
[async-trait](https://github.com/dtolnay/async-trait),
[autocfg](https://github.com/cuviper/autocfg),
[basic-toml](https://github.com/dtolnay/basic-toml),
[bhttp](https://github.com/martinthomson/ohttp),
[bitflags](https://github.com/bitflags/bitflags),
[block-buffer](https://github.com/RustCrypto/utils),
[camino](https://github.com/camino-rs/camino),
[cargo-platform](https://github.com/rust-lang/cargo),
[cc](https://github.com/rust-lang/cc-rs),
[cexpr](https://github.com/jethrogb/rust-cexpr),
[cfg-if](https://github.com/alexcrichton/cfg-if),
[chrono](https://github.com/chronotope/chrono),
[clang-sys](https://github.com/KyleMayes/clang-sys),
[core-foundation-sys](https://github.com/servo/core-foundation-rs),
[core-foundation](https://github.com/servo/core-foundation-rs),
[cpufeatures](https://github.com/RustCrypto/utils),
[crypto-common](https://github.com/RustCrypto/traits),
[digest](https://github.com/RustCrypto/traits),
[displaydoc](https://github.com/yaahc/displaydoc),
[either](https://github.com/bluss/either),
[env_logger](https://github.com/rust-cli/env_logger),
[equivalent](https://github.com/cuviper/equivalent),
[errno](https://github.com/lambda-fairy/rust-errno),
[fallible-iterator](https://github.com/sfackler/rust-fallible-iterator),
[fallible-streaming-iterator](https://github.com/sfackler/fallible-streaming-iterator),
[fastrand](https://github.com/smol-rs/fastrand),
[ffi-support](https://github.com/mozilla/ffi-support),
[find-msvc-tools](https://github.com/rust-lang/cc-rs),
[fnv](https://github.com/servo/rust-fnv),
[form_urlencoded](https://github.com/servo/rust-url),
[fs-err](https://github.com/andrewhickman/fs-err),
[futures-channel](https://github.com/rust-lang/futures-rs),
[futures-core](https://github.com/rust-lang/futures-rs),
[futures-sink](https://github.com/rust-lang/futures-rs),
[futures-task](https://github.com/rust-lang/futures-rs),
[futures-util](https://github.com/rust-lang/futures-rs),
[getrandom](https://github.com/rust-random/getrandom),
[glob](https://github.com/rust-lang/glob),
[hashbrown](https://github.com/rust-lang/hashbrown),
[hashlink](https://github.com/kyren/hashlink),
[heck](https://github.com/withoutboats/heck),
[hex](https://github.com/KokaKiwi/rust-hex),
[http](https://github.com/hyperium/http),
[httparse](https://github.com/seanmonstar/httparse),
[httpdate](https://github.com/pyfisch/httpdate),
[hyper-tls](https://github.com/hyperium/hyper-tls),
[iana-time-zone](https://github.com/strawlab/iana-time-zone),
[id-arena](https://github.com/fitzgen/id-arena),
[idna](https://github.com/servo/rust-url/),
[idna_adapter](https://github.com/hsivonen/idna_adapter),
[indexmap](https://github.com/indexmap-rs/indexmap),
[io-lifetimes](https://github.com/sunfishcode/io-lifetimes),
[itertools](https://github.com/rust-itertools/itertools),
[itoa](https://github.com/dtolnay/itoa),
[lalrpop-util](https://github.com/lalrpop/lalrpop),
[lazy_static](https://github.com/rust-lang-nursery/lazy-static.rs),
[libc](https://github.com/rust-lang/libc),
[libm](https://github.com/rust-lang/libm),
[lock_api](https://github.com/Amanieu/parking_lot),
[log](https://github.com/rust-lang/log),
[minimal-lexical](https://github.com/Alexhuszagh/minimal-lexical),
[native-tls](https://github.com/sfackler/rust-native-tls),
[num-traits](https://github.com/rust-num/num-traits),
[num_cpus](https://github.com/seanmonstar/num_cpus),
[ohttp](https://github.com/martinthomson/ohttp),
[once_cell](https://github.com/matklad/once_cell),
[parking_lot](https://github.com/Amanieu/parking_lot),
[parking_lot_core](https://github.com/Amanieu/parking_lot),
[percent-encoding](https://github.com/servo/rust-url/),
[pin-project-lite](https://github.com/taiki-e/pin-project-lite),
[pin-utils](https://github.com/rust-lang-nursery/pin-utils),
[pkg-config](https://github.com/rust-lang/pkg-config-rs),
[plain](https://github.com/randomites/plain),
[pollster](https://github.com/zesterer/pollster),
[proc-macro2](https://github.com/dtolnay/proc-macro2),
[quote](https://github.com/dtolnay/quote),
[regex-automata](https://github.com/rust-lang/regex/tree/master/regex-automata),
[regex-syntax](https://github.com/rust-lang/regex/tree/master/regex-syntax),
[regex](https://github.com/rust-lang/regex),
[rkv](https://github.com/mozilla/rkv),
[rustc-hash](https://github.com/rust-lang/rustc-hash),
[rustix](https://github.com/bytecodealliance/rustix),
[ryu](https://github.com/dtolnay/ryu),
[scopeguard](https://github.com/bluss/scopeguard),
[security-framework-sys](https://github.com/kornelski/rust-security-framework),
[security-framework](https://github.com/kornelski/rust-security-framework),
[semver](https://github.com/dtolnay/semver),
[serde](https://github.com/serde-rs/serde),
[serde_core](https://github.com/serde-rs/serde),
[serde_derive](https://github.com/serde-rs/serde),
[serde_json](https://github.com/serde-rs/json),
[serde_spanned](https://github.com/toml-rs/toml),
[sha2](https://github.com/RustCrypto/hashes),
[shlex](https://github.com/comex/rust-shlex),
[siphasher](https://github.com/jedisct1/rust-siphash),
[smallvec](https://github.com/servo/rust-smallvec),
[socket2](https://github.com/rust-lang/socket2),
[stable_deref_trait](https://github.com/storyyeller/stable_deref_trait),
[static_assertions](https://github.com/nvzqz/static-assertions-rs),
[syn](https://github.com/dtolnay/syn),
[tempfile](https://github.com/Stebalien/tempfile),
[thiserror-impl](https://github.com/dtolnay/thiserror),
[thiserror](https://github.com/dtolnay/thiserror),
[thread_local](https://github.com/Amanieu/thread_local-rs),
[toml](https://github.com/toml-rs/toml),
[toml_datetime](https://github.com/toml-rs/toml),
[toml_parser](https://github.com/toml-rs/toml),
[toml_writer](https://github.com/toml-rs/toml),
[typenum](https://github.com/paholg/typenum),
[url](https://github.com/servo/rust-url),
[utf8_iter](https://github.com/hsivonen/utf8_iter),
[uuid](https://github.com/uuid-rs/uuid),
[vcpkg](https://github.com/mcgoo/vcpkg-rs),
[version_check](https://github.com/SergioBenitez/version_check)

```
                              Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

```
-------------
## MIT License: aho-corasick, byteorder, memchr

The following text applies to code linked from these dependencies:
[aho-corasick](https://github.com/BurntSushi/aho-corasick),
[byteorder](https://github.com/BurntSushi/byteorder),
[memchr](https://github.com/BurntSushi/memchr)

```
The MIT License (MIT)

Copyright (c) 2015 Andrew Gallant

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.

```
-------------
## MIT License: bincode

The following text applies to code linked from these dependencies:
[bincode](https://github.com/servo/bincode)

```
The MIT License (MIT)

Copyright (c) 2014 Ty Overby

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

```
-------------
## MIT License: bytes

The following text applies to code linked from these dependencies:
[bytes](https://github.com/tokio-rs/bytes)

```
Copyright (c) 2018 Carl Lerche

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: cargo_metadata, winnow

The following text applies to code linked from these dependencies:
[cargo_metadata](https://github.com/oli-obk/cargo_metadata),
[winnow](https://github.com/winnow-rs/winnow)

```
Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: core_maths

The following text applies to code linked from these dependencies:
[core_maths](https://github.com/robertbastian/core_maths)

```
MIT License

Copyright (c) 2024 Robert Bastian

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
-------------
## MIT License: generic-array

The following text applies to code linked from these dependencies:
[generic-array](https://github.com/fizyk20/generic-array.git)

```
The MIT License (MIT)

Copyright (c) 2015 Bartłomiej Kamiński

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
-------------
## MIT License: goblin

The following text applies to code linked from these dependencies:
[goblin](https://github.com/m4b/goblin)

```
The MIT License (MIT)

Copyright (c) m4b 2016-2018

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

```
-------------
## MIT License: h2

The following text applies to code linked from these dependencies:
[h2](https://github.com/hyperium/h2)

```
Copyright (c) 2017 h2 authors

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: http-body

The following text applies to code linked from these dependencies:
[http-body](https://github.com/hyperium/http-body)

```
Copyright (c) 2019 Hyper Contributors

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: hyper

The following text applies to code linked from these dependencies:
[hyper](https://github.com/hyperium/hyper)

```
Copyright (c) 2014-2021 Sean McArthur

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.

```
-------------
## MIT License: libsqlite3-sys, rusqlite

The following text applies to code linked from these dependencies:
[libsqlite3-sys](https://github.com/rusqlite/rusqlite),
[rusqlite](https://github.com/rusqlite/rusqlite)

```
Copyright (c) 2014 The rusqlite developers

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.

```
-------------
## MIT License: mio

The following text applies to code linked from these dependencies:
[mio](https://github.com/tokio-rs/mio)

```
Copyright (c) 2014 Carl Lerche and other MIO contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.

```
-------------
## MIT License: nom

The following text applies to code linked from these dependencies:
[nom](https://github.com/Geal/nom)

```
Copyright (c) 2014-2019 Geoffroy Couprie

Permission is hereby granted, free of charge, to any person obtaining
a copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be
included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: ordered-float

The following text applies to code linked from these dependencies:
[ordered-float](https://github.com/reem/rust-ordered-float)

```
Copyright (c) 2015 Jonathan Reem

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: scroll

The following text applies to code linked from these dependencies:
[scroll](https://github.com/m4b/scroll)

```
The MIT License (MIT)

Copyright (c) m4b 2016

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

```
-------------
## MIT License: scroll_derive

The following text applies to code linked from these dependencies:
[scroll_derive](https://github.com/m4b/scroll)

```
MIT License

Copyright (c) 2017 

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

```
-------------
## MIT License: sharded-slab

The following text applies to code linked from these dependencies:
[sharded-slab](https://github.com/hawkw/sharded-slab)

```
Copyright (c) 2019 Eliza Weisman

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.

```
-------------
## MIT License: slab

The following text applies to code linked from these dependencies:
[slab](https://github.com/tokio-rs/slab)

```
Copyright (c) 2019 Carl Lerche

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: smawk

The following text applies to code linked from these dependencies:
[smawk](https://github.com/mgeisler/smawk)

```
MIT License

Copyright (c) 2017 Martin Geisler

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

```
-------------
## MIT License: synstructure

The following text applies to code linked from these dependencies:
[synstructure](https://github.com/mystor/synstructure)

```
Copyright 2016 Nika Layzell

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: textwrap

The following text applies to code linked from these dependencies:
[textwrap](https://github.com/mgeisler/textwrap)

```
MIT License

Copyright (c) 2016 Martin Geisler

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

```
-------------
## MIT License: tokio

The following text applies to code linked from these dependencies:
[tokio](https://github.com/tokio-rs/tokio)

```
Copyright (c) 2023 Tokio Contributors

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: tokio-native-tls, tracing, tracing-attributes, tracing-core, tracing-subscriber

The following text applies to code linked from these dependencies:
[tokio-native-tls](https://github.com/tokio-rs/tls),
[tracing-attributes](https://github.com/tokio-rs/tracing),
[tracing-core](https://github.com/tokio-rs/tracing),
[tracing-subscriber](https://github.com/tokio-rs/tracing),
[tracing](https://github.com/tokio-rs/tracing)

```
Copyright (c) 2019 Tokio Contributors

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: tokio-util

The following text applies to code linked from these dependencies:
[tokio-util](https://github.com/tokio-rs/tokio)

```
Copyright (c) 2022 Tokio Contributors

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: tower-service

The following text applies to code linked from these dependencies:
[tower-service](https://github.com/tower-rs/tower)

```
Copyright (c) 2019 Tower Contributors

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

```
-------------
## MIT License: try-lock

The following text applies to code linked from these dependencies:
[try-lock](https://github.com/seanmonstar/try-lock)

```
Copyright (c) 2018 Sean McArthur
Copyright (c) 2016 Alex Crichton

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.


```
-------------
## MIT License: want

The following text applies to code linked from these dependencies:
[want](https://github.com/seanmonstar/want)

```
Copyright (c) 2018-2019 Sean McArthur

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.


```
-------------
## MIT License: weedle2

The following text applies to code linked from these dependencies:
[weedle2](https://github.com/mozilla/uniffi-rs)

```
Copyright 2018-Present Sharad Chand
Copyright 2022 Jan-Erik Rediger

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software,
and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF
CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.

```
-------------
## ISC License: libloading

The following text applies to code linked from these dependencies:
[libloading](https://github.com/nagisa/rust_libloading/)

```
Copyright © 2015, Simonas Kazlauskas

Permission to use, copy, modify, and/or distribute this software for any purpose with or without
fee is hereby granted, provided that the above copyright notice and this permission notice appear
in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS
SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE
AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT,
NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
THIS SOFTWARE.

```
-------------
## BSD-2-Clause License: arrayref

The following text applies to code linked from these dependencies:
[arrayref](https://github.com/droundy/arrayref)

```
<!DOCTYPE html><html lang="en" dir="ltr">
  <head itemscope itemtype="http://schema.org/WebSite">
    <meta charset="utf-8">

    <link rel="stylesheet" href="https://fonts.googleapis.com/css?family=Chivo:900">
    <link rel="stylesheet" href="/assets/css/application.css?v=d6277c04899ce98b13d713186b48d110ac998d0b">
    <link rel="shortcut icon" href="/favicon.ico" type="image/x-icon">
    <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no">

    
    
    
      
      <link rel="alternate" href="https://choosealicense.com/licenses/bsd-2-clause/" hreflang="en">
    
      
      <link rel="alternate" href="https://choosealicense.com/fr/licenses/bsd-2-clause/" hreflang="fr">
    
      
      <link rel="alternate" href="https://choosealicense.com/es/licenses/bsd-2-clause/" hreflang="es">
    
      
      <link rel="alternate" href="https://choosealicense.com/pt/licenses/bsd-2-clause/" hreflang="pt">
    
      
      <link rel="alternate" href="https://choosealicense.com/zh/licenses/bsd-2-clause/" hreflang="zh">
    
      
      <link rel="alternate" href="https://choosealicense.com/ko/licenses/bsd-2-clause/" hreflang="ko">
    
      
      <link rel="alternate" href="https://choosealicense.com/ar/licenses/bsd-2-clause/" hreflang="ar">
    
    <link rel="alternate" href="https://choosealicense.com/licenses/bsd-2-clause/" hreflang="x-default">

    <!-- Begin Jekyll SEO tag v2.9.0 -->
<title>BSD 2-Clause “Simplified” License | Choose a License</title>
<meta name="generator" content="Jekyll v4.4.1" />
<meta property="og:title" content="BSD 2-Clause “Simplified” License" />
<meta property="og:locale" content="en_US" />
<meta name="description" content="A permissive license that comes in two variants, the BSD 2-Clause and BSD 3-Clause. Both have very minute differences to the MIT license." />
<meta name="twitter:description" property="og:description" content="A permissive license that comes in two variants, the BSD 2-Clause and BSD 3-Clause. Both have very minute differences to the MIT license." />
<link rel="canonical" href="https://choosealicense.com/licenses/bsd-2-clause/" />
<meta property="og:url" content="https://choosealicense.com/licenses/bsd-2-clause/" />
<meta property="og:site_name" content="Choose a License" />
<meta property="og:type" content="article" />
<meta property="article:published_time" content="2026-07-13T04:35:43+00:00" />
<meta property="article:modified_time" content="2026-07-13T04:35:43+00:00" />
<meta name="twitter:card" content="summary" />
<meta name="twitter:title" content="BSD 2-Clause “Simplified” License" />
<meta name="twitter:site" content="@github" />
<script type="application/ld+json">
{"@context":"https://schema.org","@type":"BlogPosting","dateModified":"2026-07-13T04:35:43+00:00","datePublished":"2026-07-13T04:35:43+00:00","description":"A permissive license that comes in two variants, the BSD 2-Clause and BSD 3-Clause. Both have very minute differences to the MIT license.","headline":"BSD 2-Clause “Simplified” License","mainEntityOfPage":{"@type":"WebPage","@id":"https://choosealicense.com/licenses/bsd-2-clause/"},"url":"https://choosealicense.com/licenses/bsd-2-clause/"}</script>
<!-- End Jekyll SEO tag -->

  </head>
  <body class="license">
    <div class="container">

      <div class="topbar">
        
  
  
  
  <ol>
    <li>
      
        <a href="/">Home</a> / <a href="/licenses/">Licenses</a>
      
    </li>
  </ol>
  <script type="application/ld+json">
  {
    "@context": "http://schema.org",
    "@type": "BreadcrumbList",
    "itemListElement": [{
      "@type": "ListItem",
      "position": 1,
      "item": {
        "@id": "https://choosealicense.com/",
        "name": "Home"
      }
    },{
      "@type": "ListItem",
      "position": 2,
      "item": {
        "@id": "https://choosealicense.com/licenses",
        "name": "Licenses"
      }
    },{
      "@type": "ListItem",
      "position": 3,
      "item": {
        "@id": "https://choosealicense.com/licenses/bsd-2-clause/",
        "name": "BSD 2-Clause \"Simplified\" License"
      }
    }]
  }
  </script>


        <div class="lang-cluster">
<div class="language-suggestion" hidden aria-live="polite"></div>
<details class="language-selector">
  <summary aria-label="Language" title="Language">
    <span class="language-selector-icon" aria-hidden="true"><svg width="20" height="20" viewBox="0 0 256 256" fill="currentColor"><path d="M62.4,101c-1.5-2.1-2.1-3.4-1.8-3.9c0.2-0.5,1.6-0.7,3.9-0.5c2.3,0.2,4.2,0.5,5.8,0.9c1.5,0.4,2.8,1,3.8,1.7c1,0.7,1.8,1.5,2.3,2.6c0.6,1,1,2.3,1.4,3.7c0.7,2.8,0.5,4.7-0.5,5.7c-1.1,1-2.6,0.8-4.6-0.6c-2.1-1.4-3.9-2.8-5.5-4.2C65.5,105.1,63.9,103.2,62.4,101z M40.7,190.1c4.8-2.1,9-4.2,12.6-6.4c3.5-2.1,6.6-4.4,9.3-6.8c2.6-2.3,5-4.9,7-7.7c2-2.7,3.8-5.8,5.4-9.2c1.3,1.2,2.5,2.4,3.8,3.5c1.2,1.1,2.5,2.2,3.8,3.4c1.3,1.2,2.8,2.4,4.3,3.8c1.5,1.4,3.3,2.8,5.3,4.5c0.7,0.5,1.4,0.9,2.1,1c0.7,0.1,1.7,0,3.1-0.6c1.3-0.5,3-1.4,5.1-2.8c2.1-1.3,4.7-3.1,7.9-5.4c1.6-1.1,2.4-2,2.3-2.7c-0.1-0.7-1-1-2.7-0.9c-3.1,0.1-5.9,0.1-8.3-0.1c-2.5-0.2-5-0.6-7.4-1.4c-2.4-0.8-4.9-1.9-7.5-3.4c-2.6-1.5-5.6-3.6-9.1-6.2c1-3.9,1.8-8,2.4-12.4c0.3-2.5,0.6-4.3,0.8-5.6c0.2-1.2,0.5-2.4,0.9-3.3c0.3-0.8,0.4-1.4,0.5-1.9c0.1-0.5-0.1-1-0.4-1.6c-0.4-0.5-1-1.1-1.9-1.7c-0.9-0.6-2.2-1.4-3.9-2.3c2.4-0.9,5.1-1.7,7.9-2.6c2.7-0.9,5.7-1.8,8.8-2.7c3-0.9,4.5-1.9,4.6-3.1c0.1-1.2-0.9-2.3-3.2-3.5c-1.5-0.8-2.9-1.1-4.3-0.9c-1.4,0.2-3.2,0.9-5.4,2.2c-0.6,0.4-1.8,0.9-3.4,1.6c-1.7,0.7-3.6,1.5-6,2.5c-2.4,1-5,2-7.8,3.1c-2.9,1.1-5.8,2.2-8.7,3.2c-2.9,1.1-5.7,2-8.2,2.8c-2.6,0.8-4.6,1.4-6.1,1.6c-3.8,0.8-5.8,1.6-5.9,2.4c0,0.8,1.5,1.6,4.4,2.4c1.2,0.3,2.3,0.6,3.1,0.6c0.8,0.1,1.7,0.1,2.5,0c0.8-0.1,1.6-0.3,2.4-0.5c0.8-0.3,1.7-0.7,2.8-1.1c1.6-0.8,3.9-1.7,6.9-2.8c2.9-1,6.6-2.4,11.2-4c0.9,2.7,1.4,6,1.4,9.8c0,3.8-0.4,8.1-1.4,13c-1.3-1.1-2.7-2.3-4.2-3.6c-1.5-1.3-2.9-2.6-4.3-3.9c-1.6-1.5-3.2-2.5-4.7-3c-1.6-0.5-3.4-0.5-5.5,0c-3.3,0.9-5,1.9-4.9,3.1c0,1.2,1.3,1.8,3.8,1.9c0.9,0.1,1.8,0.3,2.7,0.6c0.9,0.3,1.9,0.9,3.2,1.8c1.3,0.9,2.9,2.2,4.7,3.8c1.8,1.6,4.2,3.7,7,6.3c-1.2,2.9-2.6,5.6-4.1,8c-1.5,2.5-3.4,5-5.5,7.3c-2.2,2.4-4.7,4.8-7.7,7.2c-3,2.5-6.6,5.1-10.8,7.8c-4.3,2.8-6.5,4.7-6.5,5.6C35,192.1,37,191.7,40.7,190.1z M250.5,81.8v165.3l-111.6-36.4L10.5,253.4V76.1l29.9-10V10.4l81.2,28.7L231.3,2.6v73.1L250.5,81.8z M124.2,50.6L22.3,84.6v152.2l101.9-33.9V50.6L124.2,50.6z M219.4,71.9V19L138.1,46L219.4,71.9z M227,201.9L196.5,92L176,85.6l-30.9,90.8l18.9,5.9l5.8-18.7l31.9,10l5.7,22.3L227,201.9z M174.8,147.7l22.2,6.9l-10.9-42.9L174.8,147.7z"/></svg>
</span>
    <span class="language-selector-current">English</span>
    <span class="language-selector-caret" aria-hidden="true">▾</span>
  </summary>
  <ul class="language-menu"><li><a href="/licenses/bsd-2-clause/" lang="en" hreflang="en" rel="alternate" aria-current="true">English<span class="checkmark" aria-hidden="true">✓</span></a></li><li><a href="/fr/licenses/bsd-2-clause/" lang="fr" hreflang="fr" rel="alternate">Français</a></li><li><a href="/es/licenses/bsd-2-clause/" lang="es" hreflang="es" rel="alternate">Español</a></li><li><a href="/pt/licenses/bsd-2-clause/" lang="pt" hreflang="pt" rel="alternate">Português</a></li><li><a href="/zh/licenses/bsd-2-clause/" lang="zh" hreflang="zh" rel="alternate">中文</a></li><li><a href="/ko/licenses/bsd-2-clause/" lang="ko" hreflang="ko" rel="alternate">한국어</a></li><li><a href="/ar/licenses/bsd-2-clause/" lang="ar" hreflang="ar" rel="alternate">العربية</a></li></ul>
</details>
</div>
      </div>

      
        <h1>BSD 2-Clause “Simplified” License</h1>
      









      <div class="clearfix">

        <div class="license-body">

            

            <p>
                A permissive license that comes in two variants, the <a href="/licenses/bsd-2-clause/">BSD 2-Clause</a> and <a href="/licenses/bsd-3-clause/">BSD 3-Clause</a>. Both have very minute differences to the MIT license.
            </p>

            <div class="license-page-details">

              <table class="license-rules">
  <tr>
  
  
    
    <th class="label">Permissions</th>
  
    
    <th class="label">Conditions</th>
  
    
    <th class="label">Limitations</th>
  
  </tr>
  <tr>
    
      <td>
        <ul class="license-permissions">
          
          
            
            
              
              <li class="commercial-use">
                <span class="license-marker">✓</span>
                Commercial use
              </li>
            
          
            
            
              
              <li class="distribution">
                <span class="license-marker">✓</span>
                Distribution
              </li>
            
          
            
            
              
              <li class="modifications">
                <span class="license-marker">✓</span>
                Modification
              </li>
            
          
            
            
          
            
            
              
              <li class="private-use">
                <span class="license-marker">✓</span>
                Private use
              </li>
            
          
        </ul>
      </td>
    
      <td>
        <ul class="license-conditions">
          
          
            
            
          
            
            
              
              <li class="include-copyright">
                <span class="license-marker">ⓘ</span>
                License and copyright notice
              </li>
            
          
            
            
          
            
            
          
            
            
          
            
            
          
            
            
          
            
            
          
        </ul>
      </td>
    
      <td>
        <ul class="license-limitations">
          
          
            
            
              
              <li class="liability">
                <span class="license-marker">✕</span>
                Liability
              </li>
            
          
            
            
          
            
            
          
            
            
              
              <li class="warranty">
                <span class="license-marker">✕</span>
                Warranty
              </li>
            
          
        </ul>
      </td>
    
  </tr>
</table>


            </div>

            <pre id="license-text" dir="ltr">BSD 2-Clause License

Copyright (c) [year], [fullname]

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
</pre>

        </div> <!-- /license-body -->

<div class="sidebar">

  
  
  
  
  

  <a href="#" data-clipboard-target="#license-text" data-proofer-ignore="true" data-copied-label="Copied!" class="js-clipboard-button button">Copy license text to clipboard</a>
  
  <h3 id="suggest-this-license">Suggest this license</h3>
  <div class="repository-suggestion">
    <p>Make a pull request to suggest this license for a project that is <a href="/no-permission/">not licensed</a>. Please be polite: see if a license has already been suggested, try to suggest a license fitting for the project’s <a href="/community/">community</a>, and keep your communication with project maintainers friendly.</p>


    <div class="input-wrapper">
      <input type="text" data-license-id="BSD-2-Clause" placeholder="Enter GitHub repository URL" id="repository-url" title="status" />
      <div class="status-indicator"></div>
    </div>
  </div>
  

  <div class="how-to-apply">
    <h3 id="how-to-apply">How to apply this license</h3>
    <p>
      Create a text file (typically named LICENSE or LICENSE.txt) in the root of your source code and copy the text of the license into the file. Replace [year] with the current year and [fullname] with the name (or names) of the copyright holders.
    </p>
    <div class="note">
    <h4 id="optional-steps">Optional steps</h4>
    
    
    
    
    
    <p id="package-metadata">Add <strong><code>BSD-2-Clause</code></strong> to your project’s package description, if applicable (e.g., <a href="https://docs.npmjs.com/cli/v7/configuring-npm/package-json#license">Node.js</a>, <a href="https://guides.rubygems.org/specification-reference/#license=">Ruby</a>, and <a href="https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields">Rust</a>). This will ensure the license is displayed in package directories.</p>
    
    </div>
  </div>

  <div class="source">
    <a href="https://spdx.org/licenses/BSD-2-Clause.html">
      <span class="source-marker" aria-hidden="true">↗</span>
      Source
    </a>
  </div>

  
  <div class="projects-with-license">
    <h3>Who’s using this license?</h3>
    <ul>
      
        <li><a href="https://github.com/go-redis/redis/blob/master/LICENSE" target="_blank">go-redis</a></li>
      
        <li><a href="https://github.com/Homebrew/brew/blob/master/LICENSE.txt" target="_blank">Homebrew</a></li>
      
        <li><a href="https://github.com/ponylang/ponyc/blob/master/LICENSE" target="_blank">Pony</a></li>
      
    </ul>
  </div>
  

</div> <!-- /sidebar -->


      </div> <!-- /clearfix -->

      <footer class="site-footer clearfix">
        <nav>
          <a href="/about/">About</a>
          <a href="/terms-of-service/">Terms of Service</a>
          
          <a href="https://github.com/github/choosealicense.com/edit/gh-pages/_licenses/bsd-2-clause.txt">Help improve this page</a>
        </nav>
        <p>
          The content of this site is licensed under the <a href="https://creativecommons.org/licenses/by/3.0/">Creative Commons Attribution 3.0 Unported License</a>.
        </p>
        <div class="with-love">
          Curated with ❤️ by <a href="https://github.com">GitHub, Inc.</a> and <a href="https://github.com/github/choosealicense.com">You!</a>
        </div>
      </footer>

    </div> <!-- /container -->

    
    <script>window.annotations = {"permissions": [{"tag": "commercial-use", "label": "Commercial use", "description": "The licensed material and derivatives may be used for commercial purposes."}, {"tag": "modifications", "label": "Modification", "description": "The licensed material may be modified."}, {"tag": "distribution", "label": "Distribution", "description": "The licensed material may be distributed."}, {"tag": "private-use", "label": "Private use", "description": "The licensed material may be used and modified in private."}, {"tag": "patent-use", "label": "Patent use", "description": "This license provides an express grant of patent rights from contributors."}], "conditions": [{"tag": "include-copyright", "label": "License and copyright notice", "description": "A copy of the license and copyright notice must be included with the licensed material."}, {"tag": "include-copyright--source", "label": "License and copyright notice for source", "description": "A copy of the license and copyright notice must be included with the licensed material in source form, but is not required for binaries."}, {"tag": "document-changes", "label": "State changes", "description": "Changes made to the licensed material must be documented."}, {"tag": "disclose-source", "label": "Disclose source", "description": "Source code must be made available when the licensed material is distributed."}, {"tag": "network-use-disclose", "label": "Network use is distribution", "description": "Users who interact with the licensed material via network are given the right to receive a copy of the source code."}, {"tag": "same-license", "label": "Same license", "description": "Modifications must be released under the same license when distributing the licensed material. In some cases a similar or related license may be used."}, {"tag": "same-license--file", "label": "Same license (file)", "description": "Modifications of existing files must be released under the same license when distributing the licensed material. In some cases a similar or related license may be used."}, {"tag": "same-license--library", "label": "Same license (library)", "description": "Modifications must be released under the same license when distributing the licensed material. In some cases a similar or related license may be used, or this condition may not apply to works that use the licensed material as a library."}], "limitations": [{"tag": "trademark-use", "label": "Trademark use", "description": "This license explicitly states that it does NOT grant trademark rights, even though licenses without such a statement probably do not grant any implicit trademark rights."}, {"tag": "liability", "label": "Liability", "description": "This license includes a limitation of liability."}, {"tag": "patent-use", "label": "Patent use", "description": "This license explicitly states that it does NOT grant any rights in the patents of contributors."}, {"tag": "warranty", "label": "Warranty", "description": "This license explicitly states that it does NOT provide any warranty."}]
      };window.tooltipFormat = "%label% %category%: %description%";
      window.tooltipCategory = { "permissions": "permission", "conditions": "condition", "limitations": "limitation" };
      
      window.licenses = [
        
          {
            "title": "BSD Zero Clause License",
            "spdx_id": "0BSD"
          },
        
          {
            "title": "Academic Free License v3.0",
            "spdx_id": "AFL-3.0"
          },
        
          {
            "title": "GNU Affero General Public License v3.0",
            "spdx_id": "AGPL-3.0"
          },
        
          {
            "title": "Apache License 2.0",
            "spdx_id": "Apache-2.0"
          },
        
          {
            "title": "Artistic License 2.0",
            "spdx_id": "Artistic-2.0"
          },
        
          {
            "title": "Blue Oak Model License 1.0.0",
            "spdx_id": "BlueOak-1.0.0"
          },
        
          {
            "title": "BSD-2-Clause Plus Patent License",
            "spdx_id": "BSD-2-Clause-Patent"
          },
        
          {
            "title": "BSD 2-Clause &quot;Simplified&quot; License",
            "spdx_id": "BSD-2-Clause"
          },
        
          {
            "title": "BSD 3-Clause Clear License",
            "spdx_id": "BSD-3-Clause-Clear"
          },
        
          {
            "title": "BSD 3-Clause &quot;New&quot; or &quot;Revised&quot; License",
            "spdx_id": "BSD-3-Clause"
          },
        
          {
            "title": "BSD 4-Clause &quot;Original&quot; or &quot;Old&quot; License",
            "spdx_id": "BSD-4-Clause"
          },
        
          {
            "title": "Boost Software License 1.0",
            "spdx_id": "BSL-1.0"
          },
        
          {
            "title": "Creative Commons Attribution 4.0 International",
            "spdx_id": "CC-BY-4.0"
          },
        
          {
            "title": "Creative Commons Attribution Share Alike 4.0 International",
            "spdx_id": "CC-BY-SA-4.0"
          },
        
          {
            "title": "Creative Commons Zero v1.0 Universal",
            "spdx_id": "CC0-1.0"
          },
        
          {
            "title": "CeCILL Free Software License Agreement v2.1",
            "spdx_id": "CECILL-2.1"
          },
        
          {
            "title": "CERN Open Hardware Licence Version 2 - Permissive",
            "spdx_id": "CERN-OHL-P-2.0"
          },
        
          {
            "title": "CERN Open Hardware Licence Version 2 - Strongly Reciprocal",
            "spdx_id": "CERN-OHL-S-2.0"
          },
        
          {
            "title": "CERN Open Hardware Licence Version 2 - Weakly Reciprocal",
            "spdx_id": "CERN-OHL-W-2.0"
          },
        
          {
            "title": "Educational Community License v2.0",
            "spdx_id": "ECL-2.0"
          },
        
          {
            "title": "Eclipse Public License 1.0",
            "spdx_id": "EPL-1.0"
          },
        
          {
            "title": "Eclipse Public License 2.0",
            "spdx_id": "EPL-2.0"
          },
        
          {
            "title": "European Union Public License 1.1",
            "spdx_id": "EUPL-1.1"
          },
        
          {
            "title": "European Union Public License 1.2",
            "spdx_id": "EUPL-1.2"
          },
        
          {
            "title": "GNU Free Documentation License v1.3",
            "spdx_id": "GFDL-1.3"
          },
        
          {
            "title": "GNU General Public License v2.0",
            "spdx_id": "GPL-2.0"
          },
        
          {
            "title": "GNU General Public License v3.0",
            "spdx_id": "GPL-3.0"
          },
        
          {
            "title": "ISC License",
            "spdx_id": "ISC"
          },
        
          {
            "title": "GNU Lesser General Public License v2.1",
            "spdx_id": "LGPL-2.1"
          },
        
          {
            "title": "GNU Lesser General Public License v3.0",
            "spdx_id": "LGPL-3.0"
          },
        
          {
            "title": "LaTeX Project Public License v1.3c",
            "spdx_id": "LPPL-1.3c"
          },
        
          {
            "title": "MIT No Attribution",
            "spdx_id": "MIT-0"
          },
        
          {
            "title": "MIT License",
            "spdx_id": "MIT"
          },
        
          {
            "title": "Mozilla Public License 2.0",
            "spdx_id": "MPL-2.0"
          },
        
          {
            "title": "Microsoft Public License",
            "spdx_id": "MS-PL"
          },
        
          {
            "title": "Microsoft Reciprocal License",
            "spdx_id": "MS-RL"
          },
        
          {
            "title": "Mulan Permissive Software License, Version 2",
            "spdx_id": "MulanPSL-2.0"
          },
        
          {
            "title": "University of Illinois/NCSA Open Source License",
            "spdx_id": "NCSA"
          },
        
          {
            "title": "Open Data Commons Open Database License v1.0",
            "spdx_id": "ODbL-1.0"
          },
        
          {
            "title": "SIL Open Font License 1.1",
            "spdx_id": "OFL-1.1"
          },
        
          {
            "title": "Open Software License 3.0",
            "spdx_id": "OSL-3.0"
          },
        
          {
            "title": "PostgreSQL License",
            "spdx_id": "PostgreSQL"
          },
        
          {
            "title": "The Unlicense",
            "spdx_id": "Unlicense"
          },
        
          {
            "title": "Universal Permissive License v1.0",
            "spdx_id": "UPL-1.0"
          },
        
          {
            "title": "Vim License",
            "spdx_id": "Vim"
          },
        
          {
            "title": "Do What The F*ck You Want To Public License",
            "spdx_id": "WTFPL"
          },
        
          {
            "title": "zlib License",
            "spdx_id": "Zlib"
          }
        
      ];
      
    </script>
    <script src="/assets/js/app.js?v=d6277c04899ce98b13d713186b48d110ac998d0b"></script>
    
    <script>
      window.choosealicenseI18n = {
        current: "en",
        default: "en",
        languages: { "en": { "name": "English", "suggest": "This page is also available in **English**.", "switch": "Switch to **English**", "dismiss": "Dismiss" }, "fr": { "name": "Français", "suggest": "Cette page est aussi disponible en **français**.", "switch": "Voir en **français**", "dismiss": "Ignorer" }, "es": { "name": "Español", "suggest": "Esta página también está disponible en **español**.", "switch": "Cambiar a **español**", "dismiss": "Descartar" }, "pt": { "name": "Português", "suggest": "Esta página também está disponível em **português**.", "switch": "Mudar para **português**", "dismiss": "Dispensar" }, "zh": { "name": "中文", "suggest": "本页面也提供**中文**版本。", "switch": "切换到**中文**", "dismiss": "忽略" }, "ko": { "name": "한국어", "suggest": "이 페이지는 **한국어**로도 제공됩니다.", "switch": "**한국어**로 전환", "dismiss": "닫기" }, "ar": { "name": "العربية", "suggest": "هذه الصفحة متوفرة أيضًا **بالعربية**.", "switch": "التبديل إلى **العربية**", "dismiss": "تجاهل" } }
      };
    </script>
    <script src="/assets/js/i18n.js?v=d6277c04899ce98b13d713186b48d110ac998d0b"></script>
  </body>
</html>


```
-------------
## BSD-3-Clause License: bindgen

The following text applies to code linked from these dependencies:
[bindgen](https://github.com/rust-lang/rust-bindgen)

```
BSD 3-Clause License

Copyright (c) 2013, Jyun-Yan You
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

* Redistributions of source code must retain the above copyright notice, this
  list of conditions and the following disclaimer.

* Redistributions in binary form must reproduce the above copyright notice,
  this list of conditions and the following disclaimer in the documentation
  and/or other materials provided with the distribution.

* Neither the name of the copyright holder nor the names of its
  contributors may be used to endorse or promote products derived from
  this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

```
-------------
## Zlib License: foldhash

The following text applies to code linked from these dependencies:
[foldhash](https://github.com/orlp/foldhash)

```
Copyright (c) 2024 Orson Peters

This software is provided 'as-is', without any express or implied warranty. In
no event will the authors be held liable for any damages arising from the use of
this software.

Permission is granted to anyone to use this software for any purpose, including
commercial applications, and to alter it and redistribute it freely, subject to
the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim
    that you wrote the original software. If you use this software in a product,
    an acknowledgment in the product documentation would be appreciated but is
    not required.

2. Altered source versions must be plainly marked as such, and must not be
    misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```
-------------
## Unicode-3.0 License: icu_collections, icu_locale, icu_locale_core, icu_locale_data, icu_normalizer, icu_normalizer_data, icu_properties, icu_properties_data, icu_provider, icu_segmenter, icu_segmenter_data, litemap, potential_utf, tinystr, writeable, yoke, yoke-derive, zerofrom, zerofrom-derive, zerotrie, zerovec, zerovec-derive

The following text applies to code linked from these dependencies:
[icu_collections](https://github.com/unicode-org/icu4x),
[icu_locale](https://github.com/unicode-org/icu4x),
[icu_locale_core](https://github.com/unicode-org/icu4x),
[icu_locale_data](https://github.com/unicode-org/icu4x),
[icu_normalizer](https://github.com/unicode-org/icu4x),
[icu_normalizer_data](https://github.com/unicode-org/icu4x),
[icu_properties](https://github.com/unicode-org/icu4x),
[icu_properties_data](https://github.com/unicode-org/icu4x),
[icu_provider](https://github.com/unicode-org/icu4x),
[icu_segmenter](https://github.com/unicode-org/icu4x),
[icu_segmenter_data](https://github.com/unicode-org/icu4x),
[litemap](https://github.com/unicode-org/icu4x),
[potential_utf](https://github.com/unicode-org/icu4x),
[tinystr](https://github.com/unicode-org/icu4x),
[writeable](https://github.com/unicode-org/icu4x),
[yoke-derive](https://github.com/unicode-org/icu4x),
[yoke](https://github.com/unicode-org/icu4x),
[zerofrom-derive](https://github.com/unicode-org/icu4x),
[zerofrom](https://github.com/unicode-org/icu4x),
[zerotrie](https://github.com/unicode-org/icu4x),
[zerovec-derive](https://github.com/unicode-org/icu4x),
[zerovec](https://github.com/unicode-org/icu4x)

```
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.

```
-------------
## Optional Notice: SQLite

The following text applies to code linked from these dependencies:
[sqlite](https://www.sqlite.org/)

```
This software makes use of the 'SQLite' database engine, and we are very grateful to D. Richard Hipp and team for producing it.
```
-------------
## (MIT OR Apache-2.0) AND Unicode-3.0 License: unicode-ident

The following text applies to code linked from these dependencies:
[unicode-ident](https://github.com/dtolnay/unicode-ident)

```
                              Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

```
-------------

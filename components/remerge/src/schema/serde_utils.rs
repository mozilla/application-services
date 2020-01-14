/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// Serde doesn't support round-tripping unknown data in enums. The closest it
/// has is `#[serde(other)]`, which can only be applied to a no-data variant.
///
/// This macro generates code to do what we want, at the cost of not being as
/// efficiently represented in some serde data formats (it's identical for
/// JSON).
///
/// See `define_enum_with_unknown` for a version that
///
/// Note: Your `Unknown` variant must be named Unknown. (If you want to figure
/// out how to make the macro work without that, be my guest -- for us it does
/// not matter, and getting anything that looked nice syntactically was too hard
/// for me)
///
/// ```
/// # let _really_ignore = stringify!{
/// impl_serde_for_enum_with_unknown! {
///     MyEnum { Foo = "foo", Bar = "bar" }
/// }
/// # };
/// ```
/// // or
/// ```
/// # let _really_ignore = stringify!{
/// impl_serde_for_enum_with_unknown! {
///     MyEnum { Foo = "foo", Bar = "bar" }
///
///     // extra options here.
///     DERIVE_DISPLAY = true; // impl display
///
///     // add a from_raw(v: $Name) -> Option<Self> to `OtherTy`
///     IMPL_FROM_RAW = OtherTy;
/// }
/// # };
/// ```
macro_rules! impl_serde_for_enum_with_unknown {
    (
        $Name:ident {
            $($Variant:ident = $text:literal),+ $(,)?
        }

        $($EXTRA:tt = $val:tt;)* $(;)?
    ) => {

        impl $Name {
            pub fn is_known(&self) -> bool {
                !matches::matches!(self, $Name::Unknown(_))
            }
        }

        impl serde::Serialize for $Name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                match self {
                    $($Name::$Variant => { s.serialize_str($text) })+
                    $Name::Unknown(v) => { s.serialize_str(&v) }
                }
            }
        }

        impl<'de> serde::Deserialize<'de> for $Name {
            fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
                struct Visitor(std::marker::PhantomData<fn() -> $Name>);
                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = $Name;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str(concat!("string identifying a ", stringify!($Name)))
                    }
                    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                        match v {
                            $($text => { Ok($Name::$Variant) })+
                            other => { Ok($Name::Unknown(other.into())) }
                        }
                    }
                }
                de.deserialize_str(Visitor(std::marker::PhantomData))
            }
        }
        impl_serde_for_enum_with_unknown!(
            @extras [$(($EXTRA; $val))*]
            @repr [$Name { $($Variant = $text),+ }]
        );
    };
    // base case
    (
        @extras []
        @repr [$Name:ident { $($Variant:ident = $text:literal),+ }]
    ) => {};

    // DERIVE_DISPLAY = true
    (
        @extras [(DERIVE_DISPLAY; true) $(($EXTRA:tt; $val:tt))*]
        @repr [$Name:ident { $($Variant:ident = $text:literal),+ }]
    ) => {
        impl std::fmt::Display for $Name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $($Name::$Variant => { f.write_str($text) })+
                    $Name::Unknown(v) => { write!(f, "{} (unknown variant)", v) }
                }
            }
        }
        impl_serde_for_enum_with_unknown!(
            @extras [$(($EXTRA; $val))*]
            @repr [$Name { $($Variant = $text),+ }]
        );
    };
    (
        @extras [(IMPL_FROM_RAW; $Dest:tt) $(($EXTRA:tt; $val:tt))*]
        @repr [$Name:ident { $($Variant:ident = $text:literal),+ }]
    ) => {
        impl $Dest {
            pub(crate) fn from_raw(raw: &$Name) -> Option<Self> {
                match raw {
                    $($Name::$Variant => {
                        Some($Dest::$Variant)
                    })+
                    $Name::Unknown(v) => {
                        log::warn!(concat!("Unknown variant {} in ", stringify!($Name)), v);
                        None
                    }
                }
            }
        }
        impl_serde_for_enum_with_unknown!(
            @extras [$(($EXTRA; $val))*]
            @repr [$Name { $($Variant = $text),+ }]
        );
    };
}

/// As above but defines the enum as well. Note: Inserts a `Unknown(Box<str>)`
/// variant.
macro_rules! define_enum_with_unknown {
    (
        $(#[$m:meta])*
        pub enum $Name:ident {
            $($Variant:ident = $text:literal),+ $(,)?
        }

        $($EXTRA:tt = $val:tt;)* $(;)?
    ) => {
        $(#[$m])*
        pub enum $Name {
            Unknown(Box<str>),
            $($Variant),+
        }

        impl_serde_for_enum_with_unknown!{
            $Name {
                $($Variant = $text),+
            }
            $($EXTRA = $val;)*
        }
    };
}

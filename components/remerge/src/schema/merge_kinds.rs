/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use std::convert::TryFrom;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UntypedMerge {
    TakeNewest,
    PreferRemote,
    Duplicate,
    CompositeMember,
}

impl std::fmt::Display for UntypedMerge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UntypedMerge::TakeNewest => f.write_str("take_newest"),
            UntypedMerge::PreferRemote => f.write_str("prefer_remote"),
            UntypedMerge::Duplicate => f.write_str("duplicate"),
            UntypedMerge::CompositeMember => f.write_str("<composite member>"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextMerge {
    Untyped(UntypedMerge),
}

impl std::fmt::Display for TextMerge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextMerge::Untyped(u) => write!(f, "{}", u),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimestampMerge {
    Untyped(UntypedMerge),
    TakeMin,
    TakeMax,
}

impl std::fmt::Display for TimestampMerge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimestampMerge::Untyped(u) => write!(f, "{}", u),
            TimestampMerge::TakeMin => f.write_str("take_min"),
            TimestampMerge::TakeMax => f.write_str("take_max"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NumberMerge {
    Untyped(UntypedMerge),
    TakeMin,
    TakeMax,
    TakeSum,
}

impl std::fmt::Display for NumberMerge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NumberMerge::Untyped(u) => write!(f, "{}", u),
            NumberMerge::TakeMin => f.write_str("take_min"),
            NumberMerge::TakeMax => f.write_str("take_max"),
            NumberMerge::TakeSum => f.write_str("take_sum"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BooleanMerge {
    Untyped(UntypedMerge),
    PreferFalse,
    PreferTrue,
}

impl std::fmt::Display for BooleanMerge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BooleanMerge::Untyped(u) => write!(f, "{}", u),
            BooleanMerge::PreferFalse => f.write_str("prefer_false"),
            BooleanMerge::PreferTrue => f.write_str("prefer_true"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnyMerge {
    TakeNewest,
    PreferRemote,
    Duplicate,
    CompositeMember,

    TakeMin,
    TakeMax,
    TakeSum,
    PreferFalse,
    PreferTrue,
    // OwnGuid
    NeverMerge,
    // OwnGuid, RecordSet, UntypedMap
    SpecialCasedType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompositeMerge {
    TakeNewest,
    PreferRemote,
    Duplicate,
    // requires composite root to be a number
    TakeMin,
    TakeMax,
}

impl From<UntypedMerge> for AnyMerge {
    fn from(u: UntypedMerge) -> AnyMerge {
        match u {
            UntypedMerge::Duplicate => Self::Duplicate,
            UntypedMerge::TakeNewest => Self::TakeNewest,
            UntypedMerge::PreferRemote => Self::PreferRemote,
            UntypedMerge::CompositeMember => Self::CompositeMember,
        }
    }
}

impl PartialEq<AnyMerge> for UntypedMerge {
    fn eq(&self, m: &AnyMerge) -> bool {
        match (self, m) {
            (UntypedMerge::Duplicate, AnyMerge::Duplicate) => true,
            (UntypedMerge::TakeNewest, AnyMerge::TakeNewest) => true,
            (UntypedMerge::PreferRemote, AnyMerge::PreferRemote) => true,
            (UntypedMerge::CompositeMember, AnyMerge::CompositeMember) => true,
            _ => false,
        }
    }
}

impl PartialEq<UntypedMerge> for AnyMerge {
    fn eq(&self, m: &UntypedMerge) -> bool {
        // reverse order
        m == self
    }
}

impl TryFrom<AnyMerge> for UntypedMerge {
    type Error = ();
    fn try_from(m: AnyMerge) -> Result<Self, ()> {
        match m {
            AnyMerge::Duplicate => Ok(UntypedMerge::Duplicate),
            AnyMerge::TakeNewest => Ok(UntypedMerge::TakeNewest),
            AnyMerge::PreferRemote => Ok(UntypedMerge::PreferRemote),
            AnyMerge::CompositeMember => Ok(UntypedMerge::CompositeMember),
            _ => Err(()),
        }
    }
}
impl TryFrom<AnyMerge> for CompositeMerge {
    type Error = ();
    fn try_from(m: AnyMerge) -> Result<Self, ()> {
        match m {
            AnyMerge::Duplicate => Ok(CompositeMerge::Duplicate),
            AnyMerge::TakeNewest => Ok(CompositeMerge::TakeNewest),
            AnyMerge::PreferRemote => Ok(CompositeMerge::PreferRemote),
            AnyMerge::TakeMax => Ok(CompositeMerge::TakeMax),
            AnyMerge::TakeMin => Ok(CompositeMerge::TakeMin),
            _ => Err(()),
        }
    }
}

// macro to remove boilerplate
macro_rules! merge_boilerplate {
    // base case.
    (@type [$MergeT:ident]) => {
    };

    // @common_methods: implement an common_methods method returning Option<UntypedMerge>
    (@type [$MergeT:ident] @common_methods $($tt:tt)*) => {
        impl $MergeT {
            pub fn as_untyped(&self) -> Option<UntypedMerge> {
                #[allow(unreachable_patterns)]
                match self {
                    $MergeT::Untyped(u) => Some(*u),
                    _ => None
                }
            }
            pub fn is_composite_member(&self) -> bool {
                self.as_untyped() == Some(UntypedMerge::CompositeMember)
            }
        }
        merge_boilerplate!(@type [$MergeT] $($tt)*);
    };

    // @from_untyped: impl From<Untyped> for $MergeT
    (@type [$MergeT:ident] @from_untyped $($tt:tt)+) => {
        impl From<UntypedMerge> for $MergeT {
            #[inline]
            fn from(u: UntypedMerge) -> Self {
                $MergeT::Untyped(u)
            }
        }
        merge_boilerplate!(@type [$MergeT] $($tt)+);
    };

    // @any_equiv: impl From<$MergeT> for AnyMerge, and PartialEq
    (@type [$MergeT:ident] @any_equiv [$($T0:ident),* $(,)?] $($tt:tt)+) => {
        impl From<$MergeT> for AnyMerge {
            #[inline]
            fn from(u: $MergeT) -> Self {
                match u {
                    $MergeT::Untyped(u) => AnyMerge::from(u),
                    $($MergeT::$T0 => AnyMerge::$T0,)*
                }
            }
        }

        impl PartialEq<AnyMerge> for $MergeT {
            fn eq(&self, m: &AnyMerge) -> bool {
                #[allow(unreachable_patterns)]
                match (self, m) {
                    ($MergeT::Untyped(u), m) => u == m,
                    $(($MergeT::$T0, AnyMerge::$T0) => true,)*
                    _ => false
                }
            }
        }

        impl PartialEq<$MergeT> for AnyMerge {
            fn eq(&self, m: &$MergeT) -> bool {
                // reverse order
                m == self
            }
        }

        impl TryFrom<AnyMerge> for $MergeT {
            type Error = ();
            fn try_from(m: AnyMerge) -> Result<Self, ()> {
                if let Ok(u) = UntypedMerge::try_from(m) {
                    Ok($MergeT::Untyped(u))
                } else {
                    #[allow(unreachable_patterns)]
                    match m {
                        $(AnyMerge::$T0 => Ok($MergeT::$T0),)*
                        _ => Err(()),
                    }
                }
            }
        }
        merge_boilerplate!(@type [$MergeT] $($tt)+);
    };
    // @compare_untyped : implement PartialEq<UntypedMerge> automatically.
    (@type [$MergeT:ident] @compare_untyped $($tt:tt)*) => {
        impl PartialEq<UntypedMerge> for $MergeT {
            #[inline]
            fn eq(&self, o: &UntypedMerge) -> bool {
                #[allow(unreachable_patterns)]
                match self {
                    $MergeT::Untyped(u) => u == o,
                    _ => false,
                }
            }
        }
        impl PartialEq<$MergeT> for UntypedMerge {
            #[inline]
            fn eq(&self, o: &$MergeT) -> bool {
                o == self
            }
        }
        merge_boilerplate!(@type [$MergeT] $($tt)*);
    };

    // @compare_via_untyped [$T0, ...], implement PartialEq<$T0> for $MergeT, assuming
    // that $T0 and $MergeT only overlap in UntypedMerge impls.
    (@type [$MergeT:ident] @compare_via_untyped [$($T0:ident),* $(,)?] $($tt:tt)*) => {
        $(
            impl PartialEq<$T0> for $MergeT {
                fn eq(&self, o: &$T0) -> bool {
                    #[allow(unreachable_patterns)]
                    match (self, o) {
                        ($MergeT::Untyped(self_u), $T0::Untyped(t0_u)) => self_u == t0_u,
                        _ => false
                    }
                }
            }
            impl PartialEq<$MergeT> for $T0 {
                fn eq(&self, o: &$MergeT) -> bool {
                    PartialEq::eq(o, self)
                }
            }
        )*

        merge_boilerplate!(
            @type [$MergeT]
            $($tt)*
        );
    };

    // @compare_with [SomeTy { Enums, Vals, That, Are, The, Same }]
    (@type [$MergeT:ident] @compare_with [$T0:ident { $($Variant:ident),+ $(,)? }] $($tt:tt)*) => {
        impl PartialEq<$T0> for $MergeT {
            #[inline]
            fn eq(&self, o: &$T0) -> bool {
                #[allow(unreachable_patterns)]
                match (self, o) {
                    ($MergeT::Untyped(self_u), $T0::Untyped(t0_u)) => self_u == t0_u,
                    $(($MergeT::$Variant, $T0::$Variant) => true,)+
                    _ => false
                }
            }
        }

        impl PartialEq<$MergeT> for $T0 {
            #[inline]
            fn eq(&self, o: &$MergeT) -> bool {
                o == self
            }
        }

        merge_boilerplate!(@type [$MergeT] $($tt)*);
    };

    // @from [SomeEnum { Variants, That, Are, The, Same }]
    (@type [$MergeT:ident] @from [$T0:ident { $($Variant:ident),+ $(,)? }] $($tt:tt)*) => {
        impl From<$T0> for $MergeT {
            fn from(t: TimestampMerge) -> Self {
                match t {
                    $T0::Untyped(u) => $MergeT::Untyped(u),
                    $($T0::$Variant => $MergeT::$Variant,)+
                }
            }
        }
        merge_boilerplate!(@type [$MergeT] $($tt)*);
    }
}

merge_boilerplate!(
    @type [BooleanMerge]
    @from_untyped
    @any_equiv [PreferTrue, PreferFalse]
    @common_methods
    @compare_untyped
    @compare_via_untyped [NumberMerge, TextMerge, TimestampMerge]
);

merge_boilerplate!(
    @type [TextMerge]
    @from_untyped
    @any_equiv []
    @common_methods
    @compare_untyped
    @compare_via_untyped [NumberMerge, TimestampMerge]
);

merge_boilerplate!(
    @type [NumberMerge]
    @from_untyped
    @any_equiv [TakeMax, TakeMin, TakeSum]
    @common_methods
    @compare_untyped
    @compare_via_untyped []
    @compare_with [TimestampMerge { TakeMax, TakeMin }]
    @from [TimestampMerge { TakeMax, TakeMin }]
);

merge_boilerplate!(
    @type [TimestampMerge]
    @from_untyped
    @any_equiv [TakeMax, TakeMin]
    @common_methods
    @compare_untyped
);

//! Types representing platforms and CPU features.

#![expect(unused)]

/// Trait for getting the [`Platform`] for a specific CPU architecture.
///
/// Implemented by [`Platform`] itself, and all types representing specific CPU feature sets.
#[expect(private_bounds)]
pub(super) trait Features: Sealed + Copy {
    fn platform(self) -> Platform;
}

/// Private trait to guarantee that only types defined in this file can implement [`Features`].
trait Sealed {}

/// Enum representing CPU feature sets for a platform.
///
/// Used for runtime feature detection.
///
/// Implements [`Features`] and also provides a [`detect`] method which initializes [`Platform`]
/// via target feature detection.
///
/// [`detect`]: Self::detect
#[derive(Clone, Copy, Debug)]
pub(super) enum Platform {
    /// AVX2 feature set. Only available on x86_64.
    #[cfg(target_arch = "x86_64")]
    Avx2(Avx2),
    /// Neon feature set. Only available on aarch64.
    #[cfg(target_arch = "aarch64")]
    Neon(Neon),
    /// Exists on all platforms, but can never be created on aarch64. See [`Generic`].
    Generic(Generic),
}

impl Features for Platform {
    /// Get [`Platform`].
    ///
    /// Just returns itself.
    #[inline(always)]
    fn platform(self) -> Platform {
        self
    }
}

impl Sealed for Platform {}

/// Generic implementation.
mod generic {
    use super::*;

    /// Generic CPU feature set. Available on all platforms except aarch64.
    ///
    /// On aarch64, this type is uninhabited - a `Generic` can never be created.
    /// So [`Platform::Generic`] can never exist on aarch64, and the compiler removes code which handles it.
    ///
    /// The type still exists on aarch64, so code which uses it is still compiled and type-checked there.
    /// That means generic code is still linted, and is visible to rust-analyzer, when working on an ARM Mac.
    #[derive(Clone, Copy, Debug)]
    pub struct Generic(GenericInner);

    #[cfg(not(target_arch = "aarch64"))]
    type GenericInner = ();

    // Neon is always available on aarch64 (see `aarch64` module),
    // so the generic implementation is never needed there.
    // `Infallible` has no values, which makes `Generic` impossible to create.
    #[cfg(target_arch = "aarch64")]
    type GenericInner = std::convert::Infallible;

    impl Features for Generic {
        /// Get [`Platform`] representing generic CPU feature set.
        #[inline(always)]
        fn platform(self) -> Platform {
            Platform::Generic(self)
        }
    }

    impl Sealed for Generic {}

    impl Platform {
        /// Create [`Platform::Generic`].
        ///
        /// Private within this file. Used only in [`detect`] implementations.
        /// Not available on aarch64, where a `Generic` can't be created.
        ///
        /// [`detect`]: Self::detect
        #[cfg(not(target_arch = "aarch64"))]
        pub(super) fn generic() -> Self {
            Generic(()).platform()
        }
    }
}

pub(super) use generic::Generic;

/// x86_64 features.
/// AVX2 or generic.
#[cfg(target_arch = "x86_64")]
mod x86_64 {
    use super::*;

    /// x64_64 AVX2 CPU feature set.
    #[derive(Clone, Copy, Debug)]
    pub struct Avx2(());

    impl Features for Avx2 {
        /// Get [`Platform`] representing AVX2 CPU feature set.
        #[inline(always)]
        fn platform(self) -> Platform {
            Platform::Avx2(self)
        }
    }

    impl Sealed for Avx2 {}

    impl Platform {
        /// Detect CPU feature set for x86_64 platform.
        pub fn detect() -> Self {
            // Currently a compile-time check. Later on will be replaced by runtime detection.
            if cfg!(all(
                target_feature = "avx2",
                target_feature = "bmi2",
                target_feature = "popcnt"
            )) {
                Avx2(()).platform()
            } else {
                Platform::generic()
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
pub(super) use x86_64::Avx2;

/// aarch64 features.
/// Neon only.
#[cfg(target_arch = "aarch64")]
mod aarch64 {
    use super::*;

    // Neon is required on aarch64. In practice, this rules nothing out:
    //
    // - Every aarch64 target which has `std` enables `neon` by default.
    //   The only ones which don't are bare-metal `*-none-softfloat` targets, which can't build this crate anyway.
    // - Disabling `neon` with `-C target-feature=-neon` breaks the target's ABI.
    //   rustc already warns about that, and says it will become a hard error.
    //
    // So aarch64 builds never need the generic implementation of anything which has a Neon implementation.
    const _: () = assert!(
        cfg!(target_feature = "neon"),
        "`oxc_lexer` requires the `neon` target feature on aarch64"
    );

    /// aarch64 Neon CPU feature set.
    #[derive(Clone, Copy, Debug)]
    pub struct Neon(());

    impl Features for Neon {
        /// Get [`Platform`] representing Neon CPU feature set.
        #[inline(always)]
        fn platform(self) -> Platform {
            Platform::Neon(self)
        }
    }

    impl Sealed for Neon {}

    impl Platform {
        /// Detect CPU feature set for aarch64 platform.
        pub fn detect() -> Self {
            // Neon is always available on aarch64. See const assert above.
            Neon(()).platform()
        }
    }
}

#[cfg(target_arch = "aarch64")]
pub(super) use aarch64::Neon;

/// Fallback implementation for other platforms.
///
/// Only generic feature set is available.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
impl Platform {
    /// Detect CPU feature set for other platform.
    ///
    /// Only generic feature set is available.
    pub fn detect() -> Self {
        Platform::generic()
    }
}

/// Macro for either static or dynamic feature selection.
///
/// Arms use normal `match` syntax, but there must be exactly these 3 arms, in this order:
///
/// - `Avx2(...)`: Only compiled on x86_64.
/// - `Neon(...)`: Only compiled on aarch64.
/// - `Generic(...)`: Compiled everywhere.
///   On aarch64, it's compiled and type-checked, but can never run, so the compiler removes it.
///   See [`Generic`].
///
/// Each arm binds that platform's feature set type (e.g. `Avx2`), so it can be passed on.
/// Fallback arms (e.g. `_ => ...`) and match guards are not allowed.
///
/// Requiring all 3 arms means every call site says what to do on every platform.
/// It also makes the `match` exhaustive on every platform, without a fallback arm.
///
/// # Examples
///
/// ## Dynamic selection
///
/// ```ignore
/// use crate::pipeline::platform::{Platform, platform_select};
///
/// fn classify(platform: Platform) {
///     // Macro compiles down to a runtime match
///     platform_select!(platform, {
///         Avx2(feat) => classify_avx2(self, feat),
///         Neon(feat) => classify_neon(self, feat),
///         Generic(_) => classify_generic(self),
///     });
/// }
/// ```
///
/// ## Static selection
///
/// ```ignore
/// use crate::pipeline::platform::{Features, Platform, platform_select};
///
/// fn classify(&mut self, feat: impl Features) {
///     // Macro compiles down to a branchless select at runtime
///     platform_select!(feat, {
///         Avx2(feat) => classify_avx2(self, feat),
///         Neon(feat) => classify_neon(self, feat),
///         Generic(_) => classify_generic(self),
///     });
/// }
/// ```
///
/// ## Expansion
///
/// Macro expands to:
///
/// ```ignore
/// {
///     use crate::pipeline::platform::{Features, Platform};
///
///     let platform_or_features = platform;
///     let _ = Features::platform(platform_or_features);
///
///     match platform_or_features.platform() {
///         #[cfg(target_arch = "x86_64")]
///         Platform::Avx2(feat) => classify_avx2(self, feat),
///
///         #[cfg(target_arch = "aarch64")]
///         Platform::Neon(feat) => classify_neon(self, feat),
///
///         Platform::Generic(_) => classify_generic(self),
///     }
/// }
/// ```
macro_rules! platform_select {
    // Tokens of the arms are consumed one at a time, and copied into `[...]`.
    // `Avx2(...) =>` is rewritten to `Platform::Avx2(...) =>`, with a `#[cfg]` attribute for x86_64.
    // `Neon(...) =>` is rewritten the same way, with a `#[cfg]` attribute for aarch64.
    // `Generic(...) =>` is rewritten to `Platform::Generic(...) =>`, with no `#[cfg]` attribute.
    // All other tokens are copied unchanged, so the arm bodies are parsed by the `match` itself.
    //
    // `[no no no]` records whether the `Avx2`, `Neon` and `Generic` arms have been seen yet.
    // Each arm's rule only matches if the arms before it have been seen, and it hasn't.
    // So the arms must come in that order, once each. Anything else is a compile error.

    // Entry point
    ($platform_or_features:expr, { $($arms:tt)* }) => {
        crate::pipeline::platform::platform_select!(@munch $platform_or_features; [no no no]; []; $($arms)*)
    };

    // `Avx2` arm. Must be first.
    (
        @munch $platform_or_features:expr; [no no no]; [$($done:tt)*];
        Avx2($($pat:tt)*) => $($rest:tt)*
    ) => {
        crate::pipeline::platform::platform_select!(
            @munch $platform_or_features;
            [yes no no];
            [
                $($done)*
                #[cfg(target_arch = "x86_64")]
                Platform::Avx2($($pat)*) =>
            ];
            $($rest)*
        )
    };

    // `Neon` arm. Must follow `Avx2` arm.
    (
        @munch $platform_or_features:expr; [yes no no]; [$($done:tt)*];
        Neon($($pat:tt)*) => $($rest:tt)*
    ) => {
        crate::pipeline::platform::platform_select!(
            @munch $platform_or_features;
            [yes yes no];
            [
                $($done)*
                #[cfg(target_arch = "aarch64")]
                Platform::Neon($($pat)*) =>
            ];
            $($rest)*
        )
    };

    // `Generic` arm. Must follow `Neon` arm.
    (
        @munch $platform_or_features:expr; [yes yes no]; [$($done:tt)*];
        Generic($($pat:tt)*) => $($rest:tt)*
    ) => {
        crate::pipeline::platform::platform_select!(
            @munch $platform_or_features;
            [yes yes yes];
            [
                $($done)*
                Platform::Generic($($pat)*) =>
            ];
            $($rest)*
        )
    };

    // Any other `Name(...) =>` arm - out of order, repeated, or not one of the 3 names
    (@munch $platform_or_features:expr; $seen:tt; $done:tt; $name:ident($($pat:tt)*) => $($rest:tt)*) => {
        crate::pipeline::platform::platform_select!(@error)
    };

    // Any other `=>` - an arm whose pattern isn't `Name(...)` (e.g. `_ =>`), or which has a match guard
    (@munch $platform_or_features:expr; $seen:tt; $done:tt; => $($rest:tt)*) => {
        crate::pipeline::platform::platform_select!(@error)
    };

    // Any other token.
    //
    // Copy 1 token, and go round again.
    //
    // The arm rules only consume `Avx2(...) =>` etc, not the arm's body.
    // So the remaining tokens can't all be copied in one go. They start with the body,
    // and a `Neon(...) =>` which still needs rewriting may come after it.
    //
    // `macro_rules!` can't match "all tokens up to the next `Neon`",
    // because `$($x:tt)*` would also match `Neon`.
    // The only way to find the next arm is to step through the tokens.
    //
    // Consuming the body in the arm rules would avoid this, but would need 2 rules for each arm
    // (block body, or expression followed by `,`), and would reject comma-free bodies like `unsafe { ... }`.
    //
    // A delimited group (e.g. a block body) is a single token, so its contents are never inspected.
    // This rule must come after the arm rules and the error rules above,
    // because `$next:tt` also matches `Avx2` and `=>`.
    (@munch $platform_or_features:expr; $seen:tt; [$($done:tt)*]; $next:tt $($rest:tt)*) => {
        crate::pipeline::platform::platform_select!(
            @munch $platform_or_features;
            $seen;
            [$($done)* $next];
            $($rest)*
        )
    };

    // All tokens consumed, and all 3 arms seen
    (@munch $platform_or_features:expr; [yes yes yes]; [$($arms:tt)*];) => {{
        use crate::pipeline::platform::{Features, Platform};

        let platform_or_features = $platform_or_features;
        // Check that `platform_or_features` implements `Features`
        let _ = Features::platform(platform_or_features);

        match platform_or_features.platform() {
            $($arms)*
        }
    }};

    // All tokens consumed, but an arm is missing
    (@munch $platform_or_features:expr; $seen:tt; $done:tt;) => {
        crate::pipeline::platform::platform_select!(@error)
    };

    // Error for any arms which don't follow the rules
    (@error) => {
        compile_error!(concat!(
            "`platform_select!` must have arms `Avx2(...)`, `Neon(...)` and `Generic(...)`, ",
            "in that order, once each",
        ))
    };
}

pub(super) use platform_select;

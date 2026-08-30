//! Internal macros.

/// Generates the set-algebra impls for a bitflag newtype over `u32`.
///
/// We roll our own instead of depending on `bitflags` for one reason: the type
/// must cross the IPC boundary and appear in generated TypeScript as a plain
/// number. `bitflags`' serde representation is a struct or a string, both of
/// which would force the frontend to parse something. `#[serde(transparent)]`
/// over a `u32` costs 4 bytes per process per frame and needs no parsing.
///
/// As elsewhere, the struct itself is declared at the call site so `specta`
/// sees a concrete inner type.
macro_rules! bitflags_impls {
    (
        $name:ident: $repr:ty {
            $(
                $(#[$fmeta:meta])*
                $flag:ident = $value:expr,
            )*
        }
    ) => {
        #[allow(dead_code)]
        impl $name {
            $(
                $(#[$fmeta])*
                pub const $flag: Self = Self($value);
            )*

            #[inline]
            #[must_use]
            pub const fn empty() -> Self {
                Self(0)
            }

            #[inline]
            #[must_use]
            pub const fn bits(self) -> $repr {
                self.0
            }

            #[inline]
            #[must_use]
            pub const fn from_bits_truncate(bits: $repr) -> Self {
                Self(bits & Self::all().0)
            }

            #[inline]
            #[must_use]
            pub const fn all() -> Self {
                Self(0 $( | $value )*)
            }

            /// True when every bit in `other` is set in `self`.
            ///
            /// Note the empty set is contained in everything, matching
            /// `bitflags` semantics and ordinary set theory.
            #[inline]
            #[must_use]
            pub const fn contains(self, other: Self) -> bool {
                (self.0 & other.0) == other.0
            }

            /// True when `self` and `other` share at least one bit.
            #[inline]
            #[must_use]
            pub const fn intersects(self, other: Self) -> bool {
                (self.0 & other.0) != 0
            }

            #[inline]
            #[must_use]
            pub const fn union(self, other: Self) -> Self {
                Self(self.0 | other.0)
            }

            #[inline]
            #[must_use]
            pub const fn difference(self, other: Self) -> Self {
                Self(self.0 & !other.0)
            }

            #[inline]
            pub fn insert(&mut self, other: Self) {
                self.0 |= other.0;
            }

            #[inline]
            pub fn remove(&mut self, other: Self) {
                self.0 &= !other.0;
            }

            /// Sets or clears `flag` according to `on`.
            #[inline]
            pub fn set(&mut self, flag: Self, on: bool) {
                if on {
                    self.insert(flag);
                } else {
                    self.remove(flag);
                }
            }

            #[inline]
            #[must_use]
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }
        }

        impl core::ops::BitOr for $name {
            type Output = Self;
            #[inline]
            fn bitor(self, rhs: Self) -> Self {
                self.union(rhs)
            }
        }

        impl core::ops::BitOrAssign for $name {
            #[inline]
            fn bitor_assign(&mut self, rhs: Self) {
                self.insert(rhs);
            }
        }

        impl core::ops::BitAnd for $name {
            type Output = Self;
            #[inline]
            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }

        impl core::ops::Not for $name {
            type Output = Self;
            #[inline]
            fn not(self) -> Self {
                Self(!self.0 & Self::all().0)
            }
        }
    };
}

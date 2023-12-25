use std::{any::TypeId, cmp::Ordering};

use crate::internal::type_id::non_static_type_id;

/// The unique Specta ID for the type.
///
/// Be aware type aliases don't exist as far as Specta is concerned as they are flattened into their inner type by Rust's trait system.
/// The Specta Type ID holds for the given properties:
///  - `T::SID == T::SID`
///  - `T::SID != S::SID`
///  - `Type<T>::SID == Type<S>::SID` (unlike std::any::TypeId)
///  - `&'a T::SID == &'b T::SID` (unlike std::any::TypeId which forces a static lifetime)
///  - `Box<T> == Arc<T> == Rc<T>` (unlike std::any::TypeId)
///
// TODO: Encode the properties above into unit tests.
#[allow(clippy::derived_hash_with_manual_eq)]
#[derive(Debug, Clone, Copy, Hash)]
pub struct SpectaID {
    pub(crate) type_name: &'static str,
    pub(crate) tid: TypeId,
}

impl SpectaID {
    // TODO: Unit test this well including with non-static types.
    pub fn from<T>() -> Self {
        let type_name = std::any::type_name::<T>();
        let last_segment = type_name.rfind("::").unwrap_or(type_name.len());

        SpectaID {
            type_name: &type_name[0..last_segment],
            tid: non_static_type_id::<T>(),
        }
    }
}

// We do custom impls so the order prefers type_name over hash.
impl Ord for SpectaID {
    fn cmp(&self, other: &Self) -> Ordering {
        self.type_name
            .cmp(other.type_name)
            .then(self.tid.cmp(&other.tid))
    }
}

// We do custom impls so the order prefers type_name over hash.
impl PartialOrd<Self> for SpectaID {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// We do custom impls so equals is by SID exclusively.
impl Eq for SpectaID {}

// We do custom impls so equals is by SID exclusively.
impl PartialEq<Self> for SpectaID {
    fn eq(&self, other: &Self) -> bool {
        self.tid.eq(&other.tid)
    }
}

/// The location of the impl block for a given type. This is used for error reporting.
/// The content of it is transparent and is generated by the macros.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImplLocation(pub(crate) &'static str);

impl ImplLocation {
    /// Get the location as a string
    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

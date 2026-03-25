#![doc = include_str!("../README.md")]

mod interned;
mod interner;
mod registry;
#[cfg(feature = "serde")]
mod serde;

pub use interned::{Intern, Interned, Weak, WeakMapExt};
pub use interner::InternRef;
pub(crate) use interner::TypedInterner;
pub use registry::{INTERNERS, Interner, InternerRegistry};

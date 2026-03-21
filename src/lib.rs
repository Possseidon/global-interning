mod interned;
mod interner;
mod registry;
#[cfg(feature = "serde")]
mod serde;

pub use interned::{Intern, Interned};
pub use interner::InternRef;
pub(crate) use interner::Interner;
pub use registry::{AnyInterner, INTERNERS, InternerRegistry};

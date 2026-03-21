use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::Arc,
};

use crate::{INTERNERS, InternRef};

/// A trait alias for all types that can be interned.
///
/// - [`Hash`] and [`Eq`] to look up existing values in an internal hashmap
/// - [`Send`] and [`Sync`] since [`INTERNERS`] is shared globally across all threads
/// - `'static` since it makes no sense to intern a value with a non-`'static` lifetime
/// - `?Sized` types such as [`str`] and slices are also supported
pub trait Intern: Hash + Eq + Send + Sync + 'static {}
impl<T: ?Sized + Hash + Eq + Send + Sync + 'static> Intern for T {}

/// A value that is interned in [`INTERNERS`].
///
/// "Interned" means there is only one concrete instance for any distinct value (as dictated by
/// [`Eq`]) of type `T`.
///
/// This not only saves on memory by effectively deduplicating any interned `T` automatically but
/// also allows for a very fast [`Eq`] and [`Hash`] of [`Interned<T>`] itself. Rather than doing a
/// deep comparison/hash it can instead just compare/hash the underlying pointer.
///
/// Not only can an [`Interned`] be nested within another [`Interned`], it's actually very
/// beneficial to do so thanks to the cheap [`Eq`] and [`Hash`].
///
/// [`Interned<T>`] does not implement [`Borrow<T>`](std::borrow::Borrow) since it uses a different
/// [`Hash`] implementation that only hashes the pointer to the interned value.
#[derive(Debug)]
pub struct Interned<T: ?Sized>(Arc<T>);

impl<T: ?Sized> Clone for Interned<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: ?Sized + Intern> Default for Interned<T>
where
    Box<T>: Default,
{
    fn default() -> Self {
        Self::from_box(Default::default())
    }
}

impl<T: ?Sized> Hash for Interned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

impl<T: ?Sized> PartialEq for Interned<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: ?Sized> Eq for Interned<T> {}

impl<T: ?Sized + PartialOrd> PartialOrd for Interned<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // should be T: Ord but leaving that trait bound out here simplifies generic derives
        if self == other {
            Some(Ordering::Equal)
        } else {
            // dereference to skip Arc::partial_cmp which might also do the pointer comparison
            // (although I don't think it currently does)
            (*self.0).partial_cmp(&*other.0)
        }
    }
}

impl<T: ?Sized + Ord> Ord for Interned<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        if self == other {
            Ordering::Equal
        } else {
            // dereference to skip Arc::cmp which might also do the pointer comparison (although I
            // don't think it currently does)
            (*self.0).cmp(&*other.0)
        }
    }
}

impl<T: ?Sized> Deref for Interned<T> {
    type Target = Arc<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ?Sized, Q: ?Sized> AsRef<Q> for Interned<T>
where
    Arc<T>: AsRef<Q>,
{
    fn as_ref(&self) -> &Q {
        self.0.as_ref()
    }
}

impl<T: ?Sized + InternRef> From<&T> for Interned<T> {
    fn from(value: &T) -> Self {
        Self::from_ref(value)
    }
}

impl<T: ?Sized + InternRef> From<&mut T> for Interned<T> {
    fn from(value: &mut T) -> Self {
        Self::from_ref(value)
    }
}

impl<T: Intern> From<T> for Interned<T> {
    fn from(value: T) -> Self {
        Self::from_owned(value)
    }
}

impl<T: ?Sized + Intern> From<Box<T>> for Interned<T> {
    fn from(value: Box<T>) -> Self {
        Self::from_box(value)
    }
}

impl<T: ?Sized + Intern> From<Arc<T>> for Interned<T> {
    fn from(value: Arc<T>) -> Self {
        Self::from_arc(value)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: ?Sized + Intern> Interned<T> {
    /// Interns the given `value` via [`InternRef`] or returns an already interned value.
    ///
    /// Prefer [`Self::from_owned`] if you have an owned value and don't need it anymore.
    ///
    /// Can also be called via [`From<&T>`] or [`From<&mut T>`].
    pub fn from_ref(value: &T) -> Self
    where
        T: InternRef,
    {
        Self(
            INTERNERS
                .get_value(value)
                .unwrap_or_else(|| INTERNERS.intern(value.intern_ref())),
        )
    }

    /// Interns the given `value` or returns an already interned value.
    ///
    /// The given `value` is dropped if it was already interned.
    ///
    /// Can also be called via [`From<T>`].
    pub fn from_owned(value: T) -> Self
    where
        T: Sized,
    {
        Self(
            INTERNERS
                .get_value(&value)
                .unwrap_or_else(|| INTERNERS.intern(value.into())),
        )
    }

    /// Interns a [`Box`]ed `value` or returns an already interned value.
    ///
    /// This is a more flexible option requiring neither [`Sized`] like [`Self::from_owned`] nor
    /// [`InternRef`] like [`Self::from_ref`] but also less efficient unless you already have a
    /// [`Box`] anyway.
    ///
    /// Can also be called via [`From<Box<T>>`].
    pub fn from_box(value: Box<T>) -> Self {
        Self(
            INTERNERS
                .get_value(&*value)
                .unwrap_or_else(|| INTERNERS.intern(value.into())),
        )
    }

    /// Interns a `value` that is wrapped in an [`Arc`] or returns an already interned value.
    ///
    /// This is the most flexible option requiring neither [`Sized`] like [`Self::from_owned`] nor
    /// [`InternRef`] like [`Self::from_ref`] but also the least efficient unless you already have
    /// an [`Arc`] anyway.
    ///
    /// Can also be called via [`From<Arc<T>>`].
    pub fn from_arc(value: Arc<T>) -> Self {
        Self(
            INTERNERS
                .get_value(&*value)
                .unwrap_or_else(|| INTERNERS.intern(value)),
        )
    }
}

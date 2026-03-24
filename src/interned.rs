use std::{
    array::TryFromSliceError,
    cmp::Ordering,
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    ptr,
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

/// A value that is interned in the global [`INTERNERS`] registry.
///
/// # Interning
///
/// "Interned" means there is only one concrete instance for any distinct value - as dictated by
/// [`Eq`] - of type `T`. It is implemented as a plain [`Arc`] that is checked against a global
/// registry of [`INTERNERS`] to prevent any duplicate values.
///
/// This not only saves on memory but also allows for a very fast [`Eq`] and [`Hash`] implementation
/// on [`Interned<T>`] itself. Rather than doing a deep comparison/hash like a plain [`Arc`] would
/// do, it can instead just compare/hash the [`Arc`]'s pointer while preserving the same guarantees
/// as if it were a `T`.
///
/// Nesting [`Interned`] values is not only possible but oftentimes even beneficial thanks to the
/// aforementioned cheap [`Eq`] and [`Hash`].
///
/// # Not [`Borrow<T>`]
///
/// [`Interned<T>`] does not implement [`Borrow<T>`] since it uses a different [`Hash`]
/// implementation that only hashes the pointer to the interned value. [`Borrow<T>`] requires
/// the same [`Hash`] semantics as `T` itself.
///
/// [`Borrow<T>`]: std::borrow::Borrow
///
/// # How do I intern from an existing [`Arc`]?
///
/// While it is possible to get the internal [`Arc`] from an [`Interned`] value via [`Deref`], the
/// opposite - providing your own [`Arc`] to be interned - is **not possible**.
///
/// Reason being that an [`Arc`] can be freely coerced into a different type (e.g. `[T; N]` can be
/// coerced to `[T]` or the opposite via [`TryFrom`]) which can result in different interners
/// sharing values. Interners currently hold strong [`Arc`]s and detect unused values by checking
/// for a ref-count of 1. Shared values across different interners would thus inhibit their cleanup.
///
/// Having the interners use [`Weak`] would solve this issue but comes with a whole set of new
/// problems. It is still something to maybe look into in the future. It would also allow for
/// immediate cleanup (minus removing dead elements) although that can also be seen as a downside
/// since it can make sense to reuse values even if they are momentarily unused.
pub struct Interned<T: ?Sized>(Arc<T>);

impl<T: ?Sized> Clone for Interned<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Hidden from [`fmt::Debug`] as is the case with other smart-pointers like [`Box`] and [`Arc`].
impl<T: ?Sized + fmt::Debug> fmt::Debug for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: ?Sized> fmt::Pointer for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: ?Sized + Error> Error for Interned<T> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        (**self).source()
    }
}

impl<T: Intern + Default> Default for Interned<T> {
    fn default() -> Self {
        Self::from_owned(Default::default())
    }
}

impl<T: Intern> Default for Interned<[T]> {
    fn default() -> Self {
        Self::from_box(Default::default())
    }
}

impl Default for Interned<str> {
    fn default() -> Self {
        Self::from_box(Default::default())
    }
}

impl Default for Interned<std::ffi::OsStr> {
    fn default() -> Self {
        Self::from_box(Default::default())
    }
}

impl Default for Interned<std::ffi::CStr> {
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

impl<T: ?Sized> PartialEq<Weak<T>> for Interned<T> {
    fn eq(&self, other: &Weak<T>) -> bool {
        ptr::addr_eq(Arc::as_ptr(&self.0), other.0.as_ptr())
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

impl<T: Intern> From<Vec<T>> for Interned<[T]> {
    fn from(value: Vec<T>) -> Self {
        Self::from_box(value.into_boxed_slice())
    }
}

impl<T: ?Sized + InternRef> From<&T> for Interned<T> {
    fn from(value: &T) -> Self {
        Self::from_ref(value)
    }
}

impl<T: ?Sized + InternRef> From<&mut T> for Interned<T> {
    fn from(value: &mut T) -> Self {
        (&*value).into()
    }
}

impl<T: Intern, const N: usize> From<[T; N]> for Interned<[T]> {
    fn from(value: [T; N]) -> Self {
        Self::slice_from_array(value)
    }
}

impl<T: Intern + Clone, const N: usize> From<&[T; N]> for Interned<[T]> {
    fn from(value: &[T; N]) -> Self {
        Self::from_ref(&value[..])
    }
}

impl<T: Intern + Clone, const N: usize> From<&mut [T; N]> for Interned<[T]> {
    fn from(value: &mut [T; N]) -> Self {
        (&*value).into()
    }
}

impl<T: Intern, const N: usize> TryFrom<Box<[T]>> for Interned<[T; N]> {
    type Error = Box<[T]>;

    fn try_from(value: Box<[T]>) -> Result<Self, Self::Error> {
        value.try_into().map(Self::from_box)
    }
}

impl<T: Intern, const N: usize> TryFrom<Vec<T>> for Interned<[T; N]> {
    type Error = Vec<T>;

    fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
        value.try_into().map(Self::from_box)
    }
}

impl<'a, T: Intern + Clone, const N: usize> TryFrom<&'a [T]> for Interned<[T; N]> {
    type Error = TryFromSliceError;

    fn try_from(value: &'a [T]) -> Result<Self, Self::Error> {
        value.try_into().map(Self::from_ref)
    }
}

impl<'a, T: Intern + Clone, const N: usize> TryFrom<&'a mut [T]> for Interned<[T; N]> {
    type Error = TryFromSliceError;

    fn try_from(value: &'a mut [T]) -> Result<Self, Self::Error> {
        (&*value).try_into()
    }
}

impl<T: ?Sized + Intern, A> FromIterator<A> for Interned<T>
where
    Box<T>: FromIterator<A>,
{
    fn from_iter<I: IntoIterator<Item = A>>(iter: I) -> Self {
        Self::from_box(Box::from_iter(iter))
    }
}

impl<T: Intern> Interned<T> {
    /// Interns the given `value` or returns an already interned value.
    ///
    /// The given `value` is dropped if it was already interned.
    pub fn from_owned(value: T) -> Self {
        Self(
            INTERNERS
                .get_value(&value)
                .unwrap_or_else(|| INTERNERS.intern(value.into())),
        )
    }
}

impl<T: ?Sized + InternRef> Interned<T> {
    /// Interns the given `value` via [`InternRef`] or returns an already interned value.
    ///
    /// Prefer [`Self::from_owned`] if you have an owned value and don't need it anymore.
    pub fn from_ref(value: &T) -> Self {
        Self(
            INTERNERS
                .get_value(value)
                .unwrap_or_else(|| INTERNERS.intern(value.intern_ref().into())),
        )
    }
}

impl<T: ?Sized + Intern> Interned<T> {
    /// Interns a [`Box`]ed `value` or returns an already interned value.
    ///
    /// This is a more flexible option requiring neither [`Sized`] like [`Self::from_owned`] nor
    /// [`InternRef`] like [`Self::from_ref`] but also less efficient unless you already have a
    /// [`Box`] anyway.
    pub fn from_box(value: Box<T>) -> Self {
        Self(
            INTERNERS
                .get_value(&*value)
                .unwrap_or_else(|| INTERNERS.intern(value.into())),
        )
    }
}

impl<T: Intern> Interned<[T]> {
    /// Interns a fixed-size array `[T; N]` into an unsized slice `[T]`.
    ///
    /// [`Interned::from_owned`] cannot be used for this since it would require the given `value` to
    /// be of type `[T]` which is not possible.
    ///
    /// [`Interned::from_ref`] and [`Interned::from_box`] both work since they introduce indirection
    /// but the former requires `T` to be [`Clone`] and the latter requires eagerly wrapping the
    /// array in a [`Box`].
    pub fn slice_from_array<const N: usize>(value: [T; N]) -> Self {
        // Turbofish to make 100% sure the value is interned as [T] and **not** as [T; N].
        // Wrapping that in Interned would just unsize the already interned Arc which is wrong!
        Self(
            INTERNERS
                .get_value::<[_]>(&value)
                .unwrap_or_else(|| INTERNERS.intern::<[_]>(value.into())),
        )
    }
}

impl<T: ?Sized> Interned<T> {
    pub fn downgrade(this: &Self) -> Weak<T> {
        Weak(Arc::downgrade(&this.0))
    }
}

pub struct Weak<T: ?Sized>(std::sync::Weak<T>);

impl<T: ?Sized> Clone for Weak<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: ?Sized> fmt::Debug for Weak<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> Default for Weak<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ?Sized> Hash for Weak<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_ptr().hash(state);
    }
}

impl<T: ?Sized> PartialEq for Weak<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl<T: ?Sized> PartialEq<Interned<T>> for Weak<T> {
    fn eq(&self, other: &Interned<T>) -> bool {
        other == self
    }
}

impl<T: ?Sized> Eq for Weak<T> {}

impl<T> Weak<T> {
    pub const fn new() -> Self {
        Self(std::sync::Weak::new())
    }
}

impl<T: ?Sized> Weak<T> {
    pub fn upgrade(&self) -> Option<Interned<T>> {
        self.0.upgrade().map(Interned)
    }
}

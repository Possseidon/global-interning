use std::{
    array::TryFromSliceError,
    cmp::Ordering,
    collections::HashMap,
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    mem::ManuallyDrop,
    ops::Deref,
    sync::Arc,
};

use crate::{INTERNERS, InternRef};

/// A trait alias for all types that can be interned.
///
/// It requires all of the following:
///
/// - [`Hash`] and [`Eq`] to look up existing values in an internal hashmap
/// - [`Send`] and [`Sync`] since [`INTERNERS`] is shared globally across all threads
/// - `'static` since it makes no sense to intern a value with a non-`'static` lifetime
///
/// `?Sized` types such as [`str`] and slices are also supported.
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
/// # Cleanup
///
/// A value is automatically deinterned once all [`Interned<T>`] for that value are dropped.
///
/// # Not [`Borrow<T>`]
///
/// [`Interned<T>`] does not implement [`Borrow<T>`] since it uses a different [`Hash`]
/// implementation that only hashes the pointer to the interned value. [`Borrow<T>`] requires
/// the same [`Hash`] semantics as `T` itself.
///
/// [`Borrow<T>`]: std::borrow::Borrow
///
/// # Can I interact with the underlying [`Arc`]?
///
/// No, you can neither give your own [`Arc`] to be interned, nor can you get back the [`Arc`] from
/// an existing [`Interned`].
///
/// This crate relies on having full control over all [`Arc`]s used by [`Interned`] to ensure the
/// last [`Interned`] getting dropped also deinterning the value. If other references to the
/// underlying [`Arc`] were to exist in user code they would inhibit this cleanup.
///
/// Another reason for not being able to intern an existing [`Arc`] is the fact that an [`Arc`] can
/// be freely coerced into a different type (e.g. `[T; N]` can be coerced to `[T]` or the opposite
/// via [`TryFrom`]) which can result in different interners sharing values. This too would break
/// cleanup with the current implementation.
pub struct Interned<T: ?Sized + Intern>(ManuallyDrop<Arc<T>>);

impl<T: ?Sized + Intern> Drop for Interned<T> {
    fn drop(&mut self) {
        // SAFETY: self is not used after the Arc has been taken out
        INTERNERS.drop_value(unsafe { ManuallyDrop::take(&mut self.0) });
    }
}

impl<T: ?Sized + Intern> Clone for Interned<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Hidden from [`fmt::Debug`] as is the case with other smart-pointers like [`Box`] and [`Arc`].
impl<T: ?Sized + Intern + fmt::Debug> fmt::Debug for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self.0).fmt(f)
    }
}

impl<T: ?Sized + Intern + fmt::Display> fmt::Display for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: ?Sized + Intern> fmt::Pointer for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: ?Sized + Intern + Error> Error for Interned<T> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
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

impl<T: ?Sized + Intern> Hash for Interned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // strip metadata, similar to what ptr::addr_eq does
        let stripped_ptr = Arc::as_ptr(&self.0) as *const ();
        stripped_ptr.hash(state);
    }
}

impl<T: ?Sized + Intern> PartialEq for Interned<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: ?Sized + Intern> Eq for Interned<T> {}

impl<T: ?Sized + Intern> PartialEq<Weak<T>> for Interned<T> {
    fn eq(&self, other: &Weak<T>) -> bool {
        std::ptr::addr_eq(Arc::as_ptr(&self.0), other.0.as_ptr())
    }
}

impl<T: ?Sized + Intern + PartialOrd> PartialOrd for Interned<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            Some(Ordering::Equal)
        } else {
            // dereference to skip Arc::partial_cmp which might also do the pointer comparison
            // (although I don't think it currently does)
            (*self.0).partial_cmp(&*other.0)
        }
    }
}

impl<T: ?Sized + Intern + Ord> Ord for Interned<T> {
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

impl<T: ?Sized + Intern> Deref for Interned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ?Sized + Intern, Q: ?Sized> AsRef<Q> for Interned<T>
where
    T: AsRef<Q>,
{
    fn as_ref(&self) -> &Q {
        (**self.0).as_ref()
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

impl<T: ?Sized + Intern> Interned<T> {
    /// Wraps an [`Arc`] that is assumed to already be interned in [`INTERNERS`] in [`Interned`].
    fn new(value: Arc<T>) -> Self {
        Self(ManuallyDrop::new(value))
    }

    /// Creates a new [`Weak`] reference to this interned value.
    pub fn downgrade(this: &Self) -> Weak<T> {
        Weak(Arc::downgrade(&this.0))
    }

    /// Interns a [`Box`]ed `value` or returns an already interned value.
    ///
    /// This is a more flexible option requiring neither [`Sized`] like [`Self::from_owned`] nor
    /// [`InternRef`] like [`Self::from_ref`] but also less efficient unless you already have a
    /// [`Box`] anyway.
    pub fn from_box(value: Box<T>) -> Self {
        Self::new(
            INTERNERS
                .get_value(&*value)
                .unwrap_or_else(|| INTERNERS.intern(value.into())),
        )
    }
}

impl<T: Intern> Interned<T> {
    /// Interns the given `value` or returns an already interned value.
    ///
    /// The given `value` is dropped if it was already interned.
    pub fn from_owned(value: T) -> Self {
        Self::new(
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
        Self::new(
            INTERNERS
                .get_value(value)
                .unwrap_or_else(|| INTERNERS.intern(value.intern_ref().into())),
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
        Self::new(
            INTERNERS
                .get_value::<[_]>(&value)
                .unwrap_or_else(|| INTERNERS.intern::<[_]>(value.into())),
        )
    }
}

/// An [`Interned<T>`] that does not prevent the value from being deinterned.
///
/// It implements both [`Eq`] and [`Hash`] but those implementations come with some caveats:
///
/// Once there are no more [`Interned<T>`] for a value, all remaining [`Weak<T>`] no longer have any
/// connection to that original value. They become their own identity, only comparing equal to
/// [`Weak<T>`] that used to point to the same [`Interned<T>`]. They **do not** compare equal to a
/// newly created [`Interned<T>`] of the same original value.
///
/// While this might make it sound like [`Weak<T>`] shouldn't implement [`Eq`] and [`Hash`] at all,
/// being able to use [`Weak<T>`] as the key in a [`HashMap`] is quite useful. In fact,
/// [`WeakMapExt`] provides some extension functions for this exact use-case.
///
/// [`HashMap`]: std::collections::HashMap
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
        // strip metadata, similar to what ptr::addr_eq does
        let stripped_ptr = self.0.as_ptr() as *const ();
        stripped_ptr.hash(state);
    }
}

impl<T: ?Sized> PartialEq for Weak<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl<T: ?Sized> Eq for Weak<T> {}

impl<T: ?Sized + Intern> PartialEq<Interned<T>> for Weak<T> {
    fn eq(&self, other: &Interned<T>) -> bool {
        other == self
    }
}

impl<T> Weak<T> {
    pub const fn new() -> Self {
        Self(std::sync::Weak::new())
    }
}

impl<T: ?Sized + Intern> Weak<T> {
    /// Returns whether the [`Weak`] can no longer be upgraded.
    ///
    /// `false` does not guarantee that it is still alive, since by the time of checking it might
    /// have died already.
    pub fn dead(&self) -> bool {
        // strong_count of 1 means the Arc was already deinterned and is about to get dropped.
        // It can no longer be upgraded in this state.
        self.0.strong_count() <= 1
    }

    pub fn upgrade(&self) -> Option<Interned<T>> {
        // check for liveness before taking a lock as an optimization
        if self.dead() {
            return None;
        }

        // TypedInterner::try_deintern has a brief window between getting the strong_count and
        // removing it from the interner where the strong_count must remain at 2. Taking a lock
        // before upgrading ensures that this can't happen.

        // If this lock comes before try_deintern then it will be upgraded and prevent the deintern.
        // If this lock comes right after try_deintern before the Arc is fully dropped then the Arc
        // will have a strong_count of 1 which must be checked again.
        INTERNERS
            .get_mut::<T, _>(|_| {
                // check for liveness again since try_deintern might have just removed it
                if self.dead() {
                    return None;
                }
                self.0.upgrade()
            })
            .map(Interned::new)
    }
}

pub trait WeakMapExt {
    type Key: ?Sized;
    type Value;

    fn retain_alive(&mut self);
    fn retain_alive_and(&mut self, f: impl FnMut(Interned<Self::Key>, &mut Self::Value) -> bool);
}

impl<K: ?Sized + Intern, V, S> WeakMapExt for HashMap<Weak<K>, V, S> {
    type Key = K;
    type Value = V;

    /// Removes any entries with a [`Weak::dead`] key, retaining only alive ones.
    ///
    /// It usually makes sense to call this on maps that use [`Weak<T>`] as a key once in a while to
    /// both reclaim memory for the dead entries and improve the overall performance of the map.
    fn retain_alive(&mut self) {
        // implemented manually to skip the upgrade which requires locking
        self.retain(|key, _| !key.dead());
    }

    /// Removes any entries which are [`Weak::dead`] or for which `f` returns `false`.
    ///
    /// Prefer [`Self::retain_alive`] if `f` always returns `true` since that requires no locking
    /// (for [`Weak::upgrade`]) and is therefore faster.
    fn retain_alive_and(&mut self, mut f: impl FnMut(Interned<K>, &mut V) -> bool) {
        self.retain(|key, value| {
            if let Some(key) = key.upgrade() {
                f(key, value)
            } else {
                false
            }
        });
    }
}

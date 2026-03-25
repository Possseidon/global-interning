use std::{
    any::{Any, TypeId},
    borrow::Cow,
    mem::replace,
    sync::{Arc, LazyLock, LockResult, RwLock},
};

use crate::{Intern, TypedInterner};

/// Holds all interners and their interned values.
pub static INTERNERS: InternerRegistry = InternerRegistry {
    interners: LazyLock::new(Default::default),
};

/// The type of [`INTERNERS`].
///
/// Cannot be instantiated manually; [`INTERNERS`] is the only instance.
pub struct InternerRegistry {
    interners: LazyLock<RwLock<RegistryMap>>,
}

impl InternerRegistry {
    /// The number of interners that have been registered, including empty ones.
    pub fn len(&self) -> usize {
        LazyLock::get(&self.interners)
            .map_or(0, |interners| interners.read().expect_unpoisoned().len())
    }

    /// Whether there are no interners registered at the moment.
    pub fn is_empty(&self) -> bool {
        LazyLock::get(&self.interners)
            .is_none_or(|interners| interners.read().expect_unpoisoned().is_empty())
    }

    /// The number of interners that are registered but currently empty.
    pub fn count_empty(&self) -> usize {
        LazyLock::get(&self.interners).map_or(0, |interners| {
            interners
                .read()
                .expect_unpoisoned()
                .values()
                .filter(|interner| interner.read().expect_unpoisoned().is_empty())
                .count()
        })
    }

    /// Whether any of the registered interners are currently empty.
    pub fn any_empty(&self) -> bool {
        LazyLock::get(&self.interners).is_some_and(|interners| {
            interners
                .read()
                .expect_unpoisoned()
                .values()
                .any(|interner| interner.read().expect_unpoisoned().is_empty())
        })
    }

    /// Removes all interners that are currently empty.
    ///
    /// This take a write-lock on the **entire** registry. Prefer first checking if there is
    /// anything to clean up via e.g. [`Self::any_empty`] which only requires a read-lock.
    ///
    /// Keep in mind that it might still be beneficial to keep empty interners around for their
    /// already allocated capacity.
    ///
    /// If a removed interner was renamed via [`Interner::name_mut`] then its name is forgotten
    /// with it.
    pub fn remove_empty(&self) {
        self.retain_empty(|_| false)
    }

    /// Retains only those empty interners for which `f` returned `true`.
    ///
    /// Interners are iterated in the order that they were added.
    ///
    /// Non-empty interners cannot be removed.
    ///
    /// Always gives mutable access to the interners since retain has to take a write-lock on the
    /// entire registry anyway.
    ///
    /// # Locking
    ///
    /// Any interning operation within `f` will lead to a deadlock due to the global write-lock.
    pub fn retain_empty(&self, mut f: impl FnMut(&mut dyn Interner) -> bool) {
        if let Some(interners) = LazyLock::get(&self.interners) {
            interners.write().expect_unpoisoned().retain(|_, interner| {
                let interner = interner.get_mut().expect_unpoisoned();
                !interner.is_empty() || f(interner)
            });
        }
    }

    /// Calls `f` for all registered interners in reverse insertion order.
    ///
    /// This order was chosen for consistency with [`Self::for_each_mut`] which has its own reason.
    ///
    /// # Locking
    ///
    /// Any interning operation within `f` may lead to a deadlock.
    pub fn for_each(&self, mut f: impl FnMut(&dyn Interner)) {
        if let Some(interners) = LazyLock::get(&self.interners) {
            interners
                .read()
                .expect_unpoisoned()
                .values()
                .rev()
                .for_each(|interner| f(&*interner.read().expect_unpoisoned()));
        }
    }

    /// Calls `f` for all registered interners in reverse insertion order.
    ///
    /// This order was chosen with [`Interner::cleanup`] in mind. A nested structure of different
    /// interned types has to clean up the outermost types first since the inner ones are
    /// technically still referenced by the existing outer but unreferenced values.
    ///
    /// When such a structure is created, the inner values are usually created first. Unfortunately
    /// this is not always the case since nested values may be optional.
    ///
    /// TL;DR a single [`Self::for_each_mut`] may not necessarily result in a full cleanup but
    /// reverse order helps for the common case.
    ///
    /// # Locking
    ///
    /// Any interning operation within `f` may lead to a deadlock.
    pub fn for_each_mut(&self, mut f: impl FnMut(&mut dyn Interner)) {
        if let Some(interners) = LazyLock::get(&self.interners) {
            interners
                .read()
                .expect_unpoisoned()
                .values()
                .rev()
                .for_each(|interner| f(&mut *interner.write().expect_unpoisoned()));
        }
    }

    /// Calls `f` with the interner for `T` if it exists and forwards its result `R`.
    ///
    /// Returns [`None`] if `T` doesn't have an interner.
    ///
    /// # Locking
    ///
    /// Any interning operation within `f` may lead to a deadlock.
    pub fn get<T: ?Sized + Intern, R>(
        &self,
        f: impl FnOnce(&dyn Interner) -> Option<R>,
    ) -> Option<R> {
        if let Some(interners) = LazyLock::get(&self.interners)
            && let Some(interner) = interners.read().expect_unpoisoned().get(&TypeId::of::<T>())
        {
            f(&*interner.read().expect_unpoisoned())
        } else {
            None
        }
    }

    /// Calls `f` with the interner for `T` if it exists and forwards its result `R`.
    ///
    /// Returns [`None`] if `T` doesn't have an interner.
    ///
    /// # Locking
    ///
    /// Any interning operation within `f` may lead to a deadlock.
    pub fn get_mut<T: ?Sized + Intern, R>(
        &self,
        f: impl FnOnce(&mut dyn Interner) -> Option<R>,
    ) -> Option<R> {
        if let Some(interners) = LazyLock::get(&self.interners)
            && let Some(interner) = interners.read().expect_unpoisoned().get(&TypeId::of::<T>())
        {
            f(&mut *interner.write().expect_unpoisoned())
        } else {
            None
        }
    }

    /// Calls `f` with the interner for `T` and forwards its result `R`.
    ///
    /// If necessary an empty interner for `T` is created.
    ///
    /// # Locking
    ///
    /// Any interning operation within `f` may lead to a deadlock.
    pub fn get_or_init<T: ?Sized + Intern, R>(&self, f: impl FnOnce(&mut dyn Interner) -> R) -> R {
        if let Some(interners) = LazyLock::get(&self.interners)
            && let Some(interner) = interners.read().expect_unpoisoned().get(&TypeId::of::<T>())
        {
            f(&mut *interner.write().expect_unpoisoned())
        } else {
            f(&mut *self
                .interners
                .write()
                .expect_unpoisoned()
                .entry(TypeId::of::<T>())
                .or_insert_with(|| Box::new(RwLock::new(TypedInterner::<T>::default())))
                .get_mut()
                .expect_unpoisoned())
        }
    }

    /// Renames the interner for `T` via [`Interner::name_mut`] and returns the old name.
    ///
    /// The interner is created if it doesn't exist yet. Be careful to not remove it via e.g.
    /// [`Self::remove_empty`] since that also removes its name.
    pub fn rename<T: ?Sized + Intern>(&self, name: Cow<'static, str>) -> Cow<'static, str> {
        self.get_or_init::<T, _>(|interner| replace(interner.name_mut(), name))
    }

    /// Checks if the given `value` is already interned and, if so, returns it.
    ///
    /// This should be called before [`Self::intern`] to avoid a write-lock and having to wrap `T`
    /// in an [`Arc`] unless necessary.
    pub(crate) fn get_value<T: ?Sized + Intern>(&self, value: &T) -> Option<Arc<T>> {
        self.get::<T, _>(|interner| interner.downcast_ref().get(value))
    }

    /// Eagerly tries to intern the given `value`.
    ///
    /// If the `value` is already interned then the given `value` is dropped and the already
    /// interned [`Arc`] is returned instead.
    ///
    /// While not strictly necessary, [`Self::get`] should be called first as an optimization.
    pub(crate) fn intern<T: ?Sized + Intern>(&self, value: Arc<T>) -> Arc<T> {
        self.get_or_init::<T, _>(|interner| interner.downcast_mut().intern(value))
    }

    /// Checks if `value` is the last reference and, if so, removes it from the interner.
    ///
    /// If it was indeed the last reference, then it is dropped after the write-lock is released.
    /// This is necessary to prevent deadlocks but does mean, that there is a brief moment where the
    /// [`Arc`] has [`Arc::strong_count`] of 1. [`crate::Weak`] makes sure to not upgrade if the
    /// strong_count is 1.
    ///
    /// If it was not the last reference, then it instead dropped *while* holding the lock to ensure
    /// the [`Arc::strong_count`] is decremented in a way that ensures that the next dropped [`Arc`]
    /// is guaranteed to see the updated [`Arc::strong_count`].
    pub(crate) fn drop_value<T: ?Sized + Intern>(&self, value: Arc<T>) {
        // Return the Arc from get_mut so it gets dropped *after* the lock is released.
        self.get_mut::<T, _>(|interner| Some(interner.downcast_mut().drop_value(value)))
            .expect("interner should exist");
    }
}

/// A type-erased interner.
pub trait Interner: Any + Send + Sync {
    /// The name of the interned type.
    ///
    /// This should ideally only be used to give a user a way to distinguish interners at runtime.
    ///
    /// Defaults to [`Self::original_name`] but can be changed with [`Self::name_mut`].
    fn name(&self) -> &str;

    /// Mutable access to [`Self::name`].
    ///
    /// This can be used to provide better (e.g. less verbose) type names to show to the user or
    /// even let the user rename them manually; ideally in a persisted way.
    fn name_mut(&mut self) -> &mut Cow<'static, str>;

    /// The [`Self::name`] that is used by default which uses [`std::any::type_name`].
    ///
    /// Can be used to reset the name back to its original value.
    fn original_name(&self) -> &'static str;

    /// Resets [`Self::name`] to [`Self::original_name`].
    fn reset_name(&mut self) {
        *self.name_mut() = self.original_name().into();
    }

    /// The number of interned values, including values that are no longer used.
    fn len(&self) -> usize;

    /// Whether this interner stores any values, including ones that are no longer used.
    fn is_empty(&self) -> bool;

    /// The total number of [`Interned<T>`] that are duplicates.
    ///
    /// Only counts duplicates at the root level; any nested [`Interned<T>`] are not counted if they
    /// are already accounted for by their parent.
    ///
    /// A low number may indicate that interning hurts more than it helps.
    ///
    /// This should only be seen as an estimate since duplicate [`Interned<T>`]s can be cloned and
    /// dropped at any time from other threads.
    ///
    /// [`Interned<T>`]: crate::Interned
    fn sum_root_duplicates(&self) -> usize;

    /// Returns the number of values that can be interned without reallocating.
    ///
    /// See [`std::collections::HashSet::capacity`].
    fn capacity(&self) -> usize;

    /// See [`std::collections::HashSet::shrink_to_fit`].
    fn shrink_to_fit(&mut self);

    /// See [`std::collections::HashSet::shrink_to`].
    fn shrink_to(&mut self, min_capacity: usize);

    /// See [`std::collections::HashSet::reserve`].
    fn reserve(&mut self, additional: usize);
}

impl dyn Interner {
    /// Downcasts and [`Interner::get`]s the given `value`.
    fn downcast_ref<T: ?Sized + Intern>(&self) -> &TypedInterner<T> {
        (self as &dyn Any)
            .downcast_ref::<TypedInterner<_>>()
            .expect("interner should have to correct type")
    }

    /// Downcasts and [`Interner::intern`]s the given `value`.
    fn downcast_mut<T: ?Sized + Intern>(&mut self) -> &mut TypedInterner<T> {
        (self as &mut dyn Any)
            .downcast_mut::<TypedInterner<_>>()
            .expect("interner should have to correct type")
    }
}

/// Calls [`InternerRegistry::rename`] with the given type name [`stringify!`]ed.
///
/// By default interners use [`std::any::type_name`] which uses very verbose names.
#[macro_export]
macro_rules! rename_interner {
    ($T:ty) => {
        $crate::INTERNERS.rename::<$T>(stringify!($T).into())
    };
}

type RegistryMap = indexmap::IndexMap<TypeId, Box<RwLock<dyn Interner>>, ahash::RandomState>;

trait LockResultExt {
    type Output;

    fn expect_unpoisoned(self) -> Self::Output;
}

impl<T> LockResultExt for LockResult<T> {
    type Output = T;

    fn expect_unpoisoned(self) -> Self::Output {
        self.expect("should not be poisoned")
    }
}

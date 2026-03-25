use std::{any::type_name, borrow::Cow, sync::Arc};

use crate::{Intern, Interner};

/// Required for types to be interned with [`Interned::from_ref`](crate::Interned::from_ref).
///
/// - Has a blanket implementation over all [`Sized`] types that are [`Clone`].
/// - Also implemented for most unsized [`std`] types such as [`str`] and slices.
///
/// A blanket implementation over `Box<T>: From<&T>` would work for most unsized types but not for
/// [`Sized`] ones.
#[diagnostic::on_unimplemented(
    message = "unable to intern `{Self}` from a reference",
    note = "`Sized` types must implement `Clone` to be internable from a reference",
    note = "use an owned value via `Interned::from_owned` if `{Self}` cannot be `Clone`",
    note = "unsized types must implement `InternRef` manually"
)]
pub trait InternRef: Intern {
    fn intern_ref(&self) -> Box<Self>;
}

impl<T: Intern + Clone> InternRef for T {
    fn intern_ref(&self) -> Box<Self> {
        Box::new(self.clone())
    }
}

impl<T: Intern + Clone> InternRef for [T] {
    fn intern_ref(&self) -> Box<Self> {
        self.into()
    }
}

impl InternRef for str {
    fn intern_ref(&self) -> Box<Self> {
        self.into()
    }
}

impl InternRef for std::path::Path {
    fn intern_ref(&self) -> Box<Self> {
        self.into()
    }
}

impl InternRef for std::ffi::OsStr {
    fn intern_ref(&self) -> Box<Self> {
        self.into()
    }
}

impl InternRef for std::ffi::CStr {
    fn intern_ref(&self) -> Box<Self> {
        self.into()
    }
}

pub(crate) struct TypedInterner<T: ?Sized> {
    /// [`Arc`] instead of [`crate::Interned<T>`] since it has different [`Eq`] semantics.
    values: ahash::HashSet<Arc<T>>,
    name: Cow<'static, str>,
}

impl<T: ?Sized> Default for TypedInterner<T> {
    fn default() -> Self {
        Self {
            values: Default::default(),
            name: Self::original_name().into(),
        }
    }
}

impl<T: ?Sized + Intern> Interner for TypedInterner<T> {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut Cow<'static, str> {
        &mut self.name
    }

    fn original_name(&self) -> &'static str {
        Self::original_name()
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    fn sum_root_duplicates(&self) -> usize {
        self.values
            .iter()
            // 2 references are not yet considered a "duplicate"
            .map(|value| Arc::strong_count(value).saturating_sub(2))
            .sum()
    }

    fn capacity(&self) -> usize {
        self.values.capacity()
    }

    fn shrink_to_fit(&mut self) {
        self.values.shrink_to_fit()
    }

    fn shrink_to(&mut self, min_capacity: usize) {
        self.values.shrink_to(min_capacity)
    }

    fn reserve(&mut self, additional: usize) {
        self.values.reserve(additional)
    }
}

impl<T: ?Sized + Intern> TypedInterner<T> {
    pub(crate) fn get(&self, value: &T) -> Option<Arc<T>> {
        self.values.get(value).cloned()
    }

    pub(crate) fn intern(&mut self, value: Arc<T>) -> Arc<T> {
        if self.values.insert(value.clone()) {
            value
        } else {
            self.values
                .get(&value)
                .expect("value should already be interned")
                .clone()
        }
    }

    pub(crate) fn drop_value(&mut self, value: Arc<T>) -> Option<Arc<T>> {
        match Arc::strong_count(&value) {
            ..=1_usize => panic!("interned value should have at least 2 references"),
            2 => {
                assert!(self.values.remove(&value), "value should be interned");
                // The value is returned back to the caller with a strong_count of 1. It cannot be
                // dropped here directly since it might have nested interned values of the same type
                // which themselves also try calling this function resulting in a deadlock.
                Some(value)
            }
            // value is dropped here (by not returning it back) WHILE holding the lock on the
            // interner to ensure we definitely see the strong_count of 2
            3.. => None,
        }
    }
}

impl<T: ?Sized> TypedInterner<T> {
    fn original_name() -> &'static str {
        type_name::<T>()
    }
}

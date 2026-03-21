use std::{any::type_name, borrow::Cow, sync::Arc};

use crate::{AnyInterner, Intern};

/// Required for types to be interned with [`crate::Interned::from_ref`].
///
/// It has a blanket implementation over all [`Sized`] types that are [`Clone`].
///
/// It is also implemented for most `?Sized` [`std`] types such as [`str`] and slices.
///
/// A blanket implementation over `Box<T>: From<&T>` would work for most `?Sized` types but not for
/// sized ones.
#[diagnostic::on_unimplemented(
    message = "unable to intern `{Self}` from a reference",
    note = "`Sized` types must implement `Clone` to be internable from a reference",
    note = "use an owned value via `Interned::from_owned` if `{Self}` cannot be `Clone`",
    note = "`?Sized` types must implement `InternRef` manually"
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

pub(crate) struct Interner<T: ?Sized> {
    /// [`Arc`] instead of [`crate::Interned<T>`] since it has different [`Eq`] semantics.
    values: ahash::HashSet<Arc<T>>,
    name: Cow<'static, str>,
}

impl<T: ?Sized> Default for Interner<T> {
    fn default() -> Self {
        Self {
            values: Default::default(),
            name: type_name::<T>().into(),
        }
    }
}

impl<T: ?Sized + Intern> AnyInterner for Interner<T> {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut Cow<'static, str> {
        &mut self.name
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    fn sum_duplicates(&self) -> usize {
        self.values
            .iter()
            // 2 references are not yet considered a "duplicate"
            .map(|value| Arc::strong_count(value).saturating_sub(2))
            .sum()
    }

    fn count_unused(&self) -> usize {
        self.values
            .iter()
            .filter(|value| Self::is_unused(value))
            .count()
    }

    fn any_unused(&self) -> bool {
        self.values.iter().any(Self::is_unused)
    }

    fn cleanup(&mut self) -> usize {
        let count = self.len();
        // having exclusive access via &mut self ensure that if this is the only reference then
        // nobody else could create a new reference by cloning the Arc during this function
        self.values.retain(|value| !Self::is_unused(value));
        // the above also means len can never increase so the following cannot overflow
        count - self.len()
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

impl<T: ?Sized + Intern> Interner<T> {
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

    fn is_unused(value: &Arc<T>) -> bool {
        Arc::strong_count(value) == 1
    }
}

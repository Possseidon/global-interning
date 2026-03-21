use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Intern, Interned};

impl<T: ?Sized + Serialize> Serialize for Interned<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (**self).serialize(serializer)
    }
}

/// Could have used `T: ?Sized` and `Box<T>: Deserialize<'de>` to deserialize anything but that
/// would require [`Box`]ing [`Sized`] types unnecessarily.
impl<'de, T: Intern + Deserialize<'de>> Deserialize<'de> for Interned<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::from_owned)
    }
}

impl<'de, T: Intern + Deserialize<'de>> Deserialize<'de> for Interned<[T]> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Box::deserialize(deserializer).map(Self::from_box)
    }
}

macro_rules! impl_deserialize_from_box_for {
    ($T:ty) => {
        impl<'de> Deserialize<'de> for Interned<$T> {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Box::deserialize(deserializer).map(Self::from_box)
            }
        }
    };
}

impl_deserialize_from_box_for!(str);
impl_deserialize_from_box_for!(std::path::Path);
impl_deserialize_from_box_for!(std::ffi::OsStr);
impl_deserialize_from_box_for!(std::ffi::CStr);

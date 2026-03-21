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

impl<'de, T: ?Sized + Intern> Deserialize<'de> for Interned<T>
where
    Box<T>: Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Box::deserialize(deserializer).map(Self::from_box)
    }
}

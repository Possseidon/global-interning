# global-interning

A Rust library for [interning] any value as long as it implements [`Eq`][Eq] and [`Hash`][Hash].

Main goals:

- Reduce memory usage by eliminating duplicate values
- Cheap [`Eq`][Eq] and [`Hash`][Hash] of already `Interned` values
- Support unsized types including [`Interned<str>`][str] and [`Interned<[T]>`][slice]
- Global sharing across all threads
- `Arc`-based handle type
- Automatic deinterning of values once they are no longer used

[interning]: https://en.wikipedia.org/wiki/Interning_(computer_science)
[Eq]: https://doc.rust-lang.org/std/cmp/trait.Eq.html
[Hash]: https://doc.rust-lang.org/std/hash/trait.Hash.html
[str]: https://doc.rust-lang.org/std/primitive.str.html
[slice]: https://doc.rust-lang.org/std/primitive.slice.html

## Feature Flags

- `serde` - Implements `Serialize` and `Deserialize` for `Interned<T>`

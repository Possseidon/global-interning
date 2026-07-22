# globin

> Short for "**glob**al **in**terning".

TODO: badges

A Rust library for globally [interning] values that implement [`Eq`][Eq] and [`Hash`][Hash].

Interning means, only a single allocation for each distinct value is stored. All future requests for an equivalent value are guaranteed to reference the existing allocation, reducing memory usage and making equality, hashing and cloning inexpensive.

## Main Goals

- 📉 **Reduce memory usage**
  - Eliminate duplicate values across all threads
  - Unreferenced interned values are immediately dropped
- ⚡ **Cheap [`Clone`][Clone], [`Eq`][Eq] and [`Hash`][Hash]** for already `Interned` values
  - Everything is pointer-based
- 📚 **Unsized types**
  - Most notably [`str`][str] and [`[T]`][slice] which should be preferred over interning `String` and `Vec<T>`
- ⛓️‍💥 **`Weak` references**
  - Implement [`Eq`][Eq] and [`Hash`][Hash] unlike `Arc` (with some caveats)
  - Useful for keys in a `HashMap` - also see the `WeakMapExt` extension trait
- 🧼 **Type-erased access to interners** of individual types
  - Provides basic diagnostics such as "How many duplicates could be eliminated?"
  - Current capacity can be checked and controlled (`reserve`, `shrink_to_fit`, etc.)

While the speed should be reasonably fast, the main goal is *memory usage* - so no thread-local interner pools.

[interning]: https://en.wikipedia.org/wiki/Interning_(computer_science)
[Clone]: https://doc.rust-lang.org/std/clone/trait.Clone.html
[Eq]: https://doc.rust-lang.org/std/cmp/trait.Eq.html
[Hash]: https://doc.rust-lang.org/std/hash/trait.Hash.html
[str]: https://doc.rust-lang.org/std/primitive.str.html
[slice]: https://doc.rust-lang.org/std/primitive.slice.html

## Usage

Here is a small example for string interning:

```rust
use globin::Interned;

let a = Interned::from_ref("hello");
let b = a.clone(); // only clones an Arc, not a full string

let c = Interned::from_ref("hello"); // no new string is allocated since "hello" is already interned
assert_eq!(a, c); // only does a pointer comparison, not a full string comparison
```

Check out the documentation for other use-cases.

## Feature Flags

- `serde` - Implements `Serialize` and `Deserialize` for `Interned<T>`

## Usage of `unsafe`

This crate contains a single (and easily correctness-provable) `unsafe` block. It is not strictly necessary but should greatly help the optimizer on each access of an interned value. See [SAFETY.md](https://github.com/Possseidon/globin/blob/main/SAFETY.md) for a detailed explanation.

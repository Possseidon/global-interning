# SAFETY

There is one single use of `unsafe`; it is not technically necessary but it should help the optimizer and, maybe surprisingly, keeps the code slightly simpler. Let me explain:

`Interned<T>` is implemented as an `Arc`:

```rust
struct Interned<T>(Arc<T>);
```

This would work fine as is but: In the current design interners hold a **strong** reference to an `Interned`. This means that the reference count of the `Arc` can never drop to zero by itself once interned.

But one of the goals of this library is automatic cleanup of values once they are no longer referenced. What do we do? We implement `Drop` for `Interned`:

```rust
impl<T> Drop for Interned<T> {
    fn drop(&mut self) {
        INTERNERS.drop_value(&self.0);
    }
}
```

`INTERNERS` checks if the value has a reference count of 2 (one reference from the interner, one from the user-owned `Interned` being dropped) and we're good, right? Well... almost; but this just opened the door for a race-condition.

If two `Interned` to the same value (let's call them `A` and `B`) are dropped simultaneously on two threads then it's possible that:

1. `A` is dropped and calls `drop_value`; `B` still exists, the value remains interned, ok
2. *Before* `A` is fully dropped and decrements the `Arc`s ref-count, `B` too calls `drop_value` - but `A` still exists
3. Neither `drop_value` call actually dropped the value and it remains interned even though it should be dropped

So, where is the underlying issue? In the fact that we would have to decrement the `Arc`'s ref-count while holding a lock. But `Drop` only gives us a `&mut self` and the `Arc` is only dropped (and its ref-count decremented) *after* our drop already exited. So we're out of options? Luckily we're not:

The easiest (and also safe) solution would be to wrap the `Arc` in an `Option`:

```rust
struct Interned<T>(Option<Arc<T>>);
```

Now we can give ownership of the `Arc` to `drop_value` and let *it* take a lock and drop the `Arc` while holding the lock.

```rust
impl<T> Drop for Interned<T> {
    fn drop(&mut self) {
        INTERNERS.drop_value(self.0.take());
    }
}
```

This works perfectly fine and doesn't even introduce additional memory overhead since `Option<Arc<T>>` can be niche-optimized, but: Every time we want to do *anything* with the `Interned` we have to check that the `Option` is `Some`. The optimizer may be able to see that it's only briefly `None` during `Drop`, but there's no guarantee; avoiding these checks entirely would be nice.

Well, there is a way and it's called `ManuallyDrop`:

```rust
struct Interned<T>(ManuallyDrop<Arc<T>>);
```

`ManuallyDrop` inhibits the `Arc`'s `Drop` and gives us various ways of dropping the `Arc` manually. The most convenient being `ManuallyDrop::take`:

```rust
impl<T> Drop for Interned<T> {
    fn drop(&mut self) {
        // SAFETY: self is not used after the Arc has been taken out
        INTERNERS.drop_value(unsafe { ManuallyDrop::take(&mut self.0) });

        // doing anything with self.0 here may result in undefined behavior
    }
}
```

This converts the `ManuallyDrop<Arc<T>>` back into a plain `Arc<T>`. This is however where we need `unsafe` since the compiler can no longer guarantee that we don't do anything with `self` after we took it out.

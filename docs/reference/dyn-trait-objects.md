# Dyn Trait Objects

See also: [Types](./types.md), [Generics](./generics.md), [Expressions](./expressions.md), [Memory](./memory.md)

This page describes runtime trait objects using `dyn Trait`.

## What `dyn Trait` means

`dyn Trait` is a runtime-dispatch type. Values are stored as a fat pointer:

- `data_ptr`: pointer to heap-allocated concrete value
- `vtable_ptr`: pointer to method table for `(Trait, ConcreteType)`

Method calls like `h.method(...)` are dispatched through the vtable at runtime.

## Basic usage

```aic
trait Handler {
    fn value(self: Self) -> Int;
}

struct PlusOne { base: Int }

impl Handler[PlusOne] {
    fn value(self: PlusOne) -> Int {
        self.base + 1
    }
}

fn score(h: dyn Handler) -> Int {
    h.value()
}

fn main() -> Int {
    let h: dyn Handler = PlusOne { base: 41 };
    score(h)
}
```

## Collections and wrappers

`dyn Trait` can be used inside container types such as `Vec` and `Option`.

```aic
import std.vec;

trait Handler {
    fn value(self: Self) -> Int;
}

struct A { base: Int }
struct B { base: Int }

impl Handler[A] {
    fn value(self: A) -> Int { self.base + 1 }
}

impl Handler[B] {
    fn value(self: B) -> Int { self.base + 2 }
}

fn total(v: Option[dyn Handler]) -> Int {
    match v {
        Some(h) => h.value(),
        None => 0,
    }
}

fn build() -> Vec[dyn Handler] {
    let mut items: Vec[dyn Handler] = vec.new_vec();
    items = vec.push(items, A { base: 10 });
    items = vec.push(items, B { base: 20 });
    items
}
```

## Object-safety rules

A trait can be used as `dyn Trait` only when it is object-safe:

- Trait-level generics are not allowed.
- Trait methods must not be generic.
- The first method parameter must be `Self`.
- `Self` may only appear in the receiver position.
- `Self` may not appear in return types.

Violations are reported at compile time.

## Current impl syntax note

To implement a non-generic trait for a concrete type, use:

```aic
impl TraitName[ConcreteType] {
    // methods
}
```

This is the current supported trait-impl surface for dyn-dispatchable traits.

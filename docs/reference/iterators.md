# Iterators

AICore supports iterator-driven `for-in` loops and lazy adapter pipelines through `std.iterator`.

## Import

```aic
import std.iterator;
```

Non-`std.*` entry modules automatically load `std.iterator`, so `for item in collection` works for supported collections without an explicit import. Add the import when you call iterator APIs directly (`iter`, `map`, `filter`, `collect`, ...).

## Iterator Protocol

`std.iterator` defines:

- `Iterator[State, Item]` with `next(self: State) -> IterStep[Item, State]`
- `Iterable[Collection, Item, State]` with `iter(self: Collection) -> LazyIter[Item, State]`
- `LazyIterOps[...]` for adapter methods

Built-in implementations exist for:

- `Vec[T]`
- `Map[K, V]`
- `Set[T]`
- `Deque[T]`

## For-In Desugaring

`for value in source { ... }` lowers to a compiler-generated iterator loop that:

1. Calls `aic_for_into_iter(source)`
2. Repeatedly calls `aic_for_next_iter(iter_state)`
3. Breaks on `None`, executes loop body on `Some(value)`

This supports both collections (`iter`) and iterator state types (`next`).

## Lazy Adapters

`LazyIterOps` adapters are lazy and chainable:

- `map`
- `filter`
- `take`
- `skip`
- `enumerate`
- `zip`
- `chain`
- `collect` (materialize to `Vec`)

Example:

```aic
import std.iterator;
import std.vec;

fn gt_two(x: Int) -> Bool { x > 2 }
fn double(x: Int) -> Int { x * 2 }

fn main() -> Int {
    let mut v: Vec[Int] = vec.new_vec();
    v = vec.push(v, 1);
    v = vec.push(v, 2);
    v = vec.push(v, 3);
    v = vec.push(v, 4);

    let out = v.iter().map(double).filter(gt_two).take(2).collect();

    let mut total = 0;
    for item in out {
        total = total + item;
    };
    total
}
```

## Custom Iterator Example

Custom types can implement `Iterator` and participate in `for-in`:

```aic
import std.iterator;

struct Counter {
    current: Int,
    limit: Int,
}

impl Iterator[Counter, Int] {
    fn next[T, U](self: Counter) -> IterStep[Int, Counter] {
        if self.current < self.limit {
            IterStep {
                item: Some(self.current),
                iter: Counter {
                    current: self.current + 1,
                    limit: self.limit,
                },
            }
        } else {
            IterStep {
                item: None(),
                iter: self,
            }
        }
    }
}

fn main() -> Int {
    let mut total = 0;
    for value in Counter { current: 0, limit: 5 } {
        total = total + value;
    };
    total
}
```

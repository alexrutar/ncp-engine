> [!INFO]
> This is a fork of [helix-editor/nucleo](https://github.com/helix-editor/nucleo) designed to implement some minor API changes, bug fixes, and many performance improvements.

This is a fork of [`nucleo`](https://crates.io/crates/nucleo) with minor API changes, bug fixes, scoring improvements, and many performance improvements.
This crate is primarily used to better support the requirements in [`nucleo-picker`](https://github.com/autobib/nucleo-picker) but other crates which depend on `nucleo` may find this useful as well.

This fork branched at commit hash `5b74652e482f7c07d827f18c6d21e7540c242c69`.

The current differences are:
- Updated dependencies and migrated to 2024 edition
- Reverted commit `61f2a4e48270174dd3789485b2991fbc0a32fbc8`, so we do not sort results on the empty pattern
- `Injector::extend` now accepts any iterator.
  The new implementation uses `size_hint` internally.
- `Injector::get` and `Injector::get_unchecked` were renamed to `Injector::get_item` and `Injector::get_item_unchecked`.
- `ncp_matcher::chars::normalize` was renamed to `normalize_latin`.
- Added `DetachedItem`, an owned, cheaply cloneable item handle.
- Added conversion from `Utf32Str` to `Utf32String`.
- Removed the `Send + Sync + 'static` bounds from the `Nucleo` and `Snapshot` type declarations.
  (Unsurprisingly, the bounds are still required to actually call `Nucleo` and `Snapshot` methods.)
- Added support for targets with 32-bit atomics but no 64-bit atomics.
- Use Rayon `redue` for worker matching instead of a shared atomic rejection counter to reduce contention between threads.
- Unified `prefer_prefix` scoring across matching paths using a bounded bonus based on the first selected index.

## Benchmarks

In order to download the benchmark data, run `python3 benches/prepare_data.py`.
This requires Python 3.9+ and curl.

To run benchmarks, run:

```sh
cargo bench --bench insertion
cargo bench --bench matching
```

Matching benchmarks cover corpus scoring, complete engine searches, append/backspace edits, and newly injected items.
By default, benchmarks run with 100,000 items and worker counts of 1 and up to 8.
This can be overridden with `NCP_BENCH_SIZE=10000|100000|full` and `NCP_BENCH_THREADS=1,4`.

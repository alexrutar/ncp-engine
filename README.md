> [!WARNING]
> This is an unstable fork of [helix-editor/nucleo](https://github.com/helix-editor/nucleo).
> You probably don't want to use this crate; there are no plans for stable releases and could change substantially between minor version changes.

This is a fork of [`nucleo`](https://crates.io/crates/nucleo) which has some modifications to better support the requirements in [`nucleo-picker`](https://github.com/autobib/nucleo-picker).

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
  (The bounds are still required to actually call `Nucleo` and `Snapshot` methods.)
- Added support for targets with 32-bit atomics but no 64-bit atomics.

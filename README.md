> [!WARNING]
> This is an unstable upstream fork of [helix-editor/nucleo](https://github.com/helix-editor/nucleo).
> You probably don't want to use this crate; it will never have stable releases and could break at any moment.

This is a fork of [`nucleo`](https://crates.io/crates/nucleo) which has some modifications to better support the requirements in [`nucleo-picker`](https://github.com/autobib/nucleo-picker).

This fork branched at commit hash `5b74652e482f7c07d827f18c6d21e7540c242c69`.

The current differences are:
- Updated dependencies and migrated to 2024 edition
- Renamed `Nucleo::extend` to `Nucleo::extend_exact` to better reflect implementation.

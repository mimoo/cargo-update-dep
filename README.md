# cargo-update-dep

Command line interface to upgrade a Rust specific dependency to a specific version in `Cargo.toml` and `Cargo.lock` files.

## Usage

To update the package `lazy_static` from version `1.3.0` to version `1.4.0`:

```
cargo update-dep -p lazy_static -v 1.3.0 -n 1.4.0
```


## Installation

```
cargo install cargo-update-dep
```
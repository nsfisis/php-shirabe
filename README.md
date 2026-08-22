# Shirabe

![Shirabe logo](./logo.svg)

Shirabe is a port of [Composer](https://github.com/composer/composer), the dependency manager for PHP, written in Rust.

It aims at 100% compatibility with Composer, including the plugin API.

*WORK IN PROGRESS*: while full compatibility is the goal, the project is at an early stage and still has many incompatibilities and bugs. See [known incompatibilities](./docs/known-incompatibilities.md) for the differences that will remain intentionally.


## Plugins

Shirabe runs Composer plugins using the system-provided real PHP. The plugin API is partially implemented.


## Requirements

Building Shirabe requires:

* Rust toolchain
* Git
* PHP
* Composer
* Just (optional)
* Nix (optional)
* Direnv (optional)

Nix flake supported; use `nix develop` to enter the development shell.
TODO: support `nix build`


## Build

```
$ git submodule update --init
$ composer install --no-dev --working-dir=composer
$ cargo build --release
```


## Test

```
$ cargo test
```


## License

See [LICENSE.md](./LICENSE.md).

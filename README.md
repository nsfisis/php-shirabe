# Shirabe

![Shirabe logo](./assets/logo/logo.svg)

Shirabe is a port of [Composer](https://github.com/composer/composer), the dependency manager for PHP, written in Rust.

It aims at 100% compatibility with Composer, including the plugin API.

*WORK IN PROGRESS*: while full compatibility is the goal, the project is at an early stage and still has many incompatibilities and bugs. See [known incompatibilities](./docs/known-incompatibilities.md) for the known differences.


## Plugins

Shirabe runs Composer plugins using the system-provided real PHP. The plugin API is partially implemented. The following plugins are verified to work:

* [phpstan/extension-installer](https://github.com/phpstan/extension-installer)


## Requirements

Running Shirabe requires:

* Linux (see [platform support](./docs/known-incompatibilities.md#platform-support))
* PHP, found on `PATH` or pointed at by the `PHP_BINARY` environment variable

Shirabe is tested with PHP 8.5. Older PHP versions have not been tested.

Building Shirabe requires:

* Rust toolchain
* Git
* PHP
* Composer
* Just (optional)
* Nix (optional)
* Direnv (optional)

Nix flake supported; use `nix develop` to enter the development shell.


## Build

```
$ git submodule update --init
$ composer install --no-dev --working-dir=composer
$ cargo build --release
```

### Nix

```
$ nix build
```


## Test

```
$ just fetch-fixtures
$ cargo test
```

`just fetch-fixtures` fetches third-party Composer plugins.


## License

See [LICENSE.md](./LICENSE.md).

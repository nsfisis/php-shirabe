# Shirabe

![Shirabe logo](./logo.svg)

Shirabe is a port of [Composer](https://github.com/composer/composer), the dependency manager for PHP, written in Rust.

It aims at 100% compatibility with Composer, including the plugin API.

*WORK IN PROGRESS*: while full compatibility is the goal, the project is at an early stage and still has many incompatibilities and bugs. See [known incompatibilities](./docs/known-incompatibilities.md) for the differences that will remain intentionally.


## Build

```
$ git submodule update --init
$ cargo build --release
```


## Test

```
$ cargo test
```


## License

See [LICENSE.md](./LICENSE.md).

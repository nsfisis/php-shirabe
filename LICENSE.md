# License

## Shirabe

Shirabe is licensed under The MIT License:

---

MIT License

Copyright 2026 nsfisis

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the “Software”), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

---

## Composer and its dependencies.

Shirabe is a port of Composer v2.9.7 with its dependencies, all of which are
under The MIT License. Every crate that contains ported code carries the
license of the package it is ported from:

| Crate                                                                       | Ported from                                 |
| --------------------------------------------------------------------------- | ------------------------------------------- |
| [`shirabe`](crates/shirabe/LICENSE)                                         | composer/composer, symfony/console, psr/log |
| [`shirabe-ca-bundle`](crates/shirabe-ca-bundle/LICENSE)                     | composer/ca-bundle                          |
| [`shirabe-class-map-generator`](crates/shirabe-class-map-generator/LICENSE) | composer/class-map-generator                |
| [`shirabe-metadata-minifier`](crates/shirabe-metadata-minifier/LICENSE)     | composer/metadata-minifier                  |
| [`shirabe-pcre`](crates/shirabe-pcre/LICENSE)                               | composer/pcre                               |
| [`shirabe-php-rpc`](crates/shirabe-php-rpc/LICENSE)                         | composer/xdebug-handler                     |
| [`shirabe-seld-json-lint`](crates/shirabe-seld-json-lint/LICENSE)           | seld/jsonlint                               |
| [`shirabe-seld-signal`](crates/shirabe-seld-signal/LICENSE)                 | seld/signal-handler                         |
| [`shirabe-semver`](crates/shirabe-semver/LICENSE)                           | composer/semver                             |
| [`shirabe-spdx-licenses`](crates/shirabe-spdx-licenses/LICENSE)             | composer/spdx-licenses                      |
| [`shirabe-symfony-console`](crates/shirabe-symfony-console/LICENSE)         | symfony/console                             |
| [`shirabe-symfony-filesystem`](crates/shirabe-symfony-filesystem/LICENSE)   | symfony/filesystem                          |
| [`shirabe-symfony-finder`](crates/shirabe-symfony-finder/LICENSE)           | symfony/finder                              |
| [`shirabe-symfony-process`](crates/shirabe-symfony-process/LICENSE)         | symfony/process                             |
| [`shirabe-symfony-string`](crates/shirabe-symfony-string/LICENSE)           | symfony/string                              |

The Shirabe executable embeds these PHP sources into its own binary as the
Composer runtime bundle ([docs/dev/composer-runtime-bundle.md](docs/dev/composer-runtime-bundle.md)).
The bundle has the `LICENSE` file of every package.

## PHP

The [`shirabe-php-src`](crates/shirabe-php-src/LICENSE) crate contains a Rust
port from the C implementation of PHP's standard library in
[php-src](https://github.com/php/php-src). It carries the PHP License 4.0
(3-clause BSD), and the zlib license for `src/standard/strnatcmp.rs`.

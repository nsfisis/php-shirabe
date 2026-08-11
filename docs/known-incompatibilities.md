# Known Incompatibilities

NOTE: This is not an exhaustive list. Shirabe is in early development and there are still a number of significant incompatibilities with Composer that are not documented here yet.


## Default Home/Cache/Data Directories

To avoid conflicting with an existing Composer installation, Shirabe's default system directories
use `shirabe`/`Shirabe` instead of `composer`/`Composer`.

| Purpose   | OS            | Composer default                | Shirabe default                |
| --------- | ------------- | ------------------------------- | ------------------------------ |
| Home dir  | Unix, XDG     | `$XDG_CONFIG_HOME/composer`     | `$XDG_CONFIG_HOME/shirabe`     |
| Home dir  | Unix, non-XDG | `$HOME/.composer`               | `$HOME/.shirabe`               |
| Home dir  | Windows       | `%APPDATA%/Composer`            | `%APPDATA%/Shirabe`            |
| Cache dir | Unix, XDG     | `$XDG_CACHE_HOME/composer`      | `$XDG_CACHE_HOME/shirabe`      |
| Cache dir | macOS         | `$HOME/Library/Caches/composer` | `$HOME/Library/Caches/shirabe` |
| Cache dir | Windows       | `%LOCALAPPDATA%/Composer`       | `%LOCALAPPDATA%/Shirabe`       |
| Data dir  | Unix, XDG     | `$XDG_DATA_HOME/composer`       | `$XDG_DATA_HOME/shirabe`       |

The following are intentionally left unchanged for ecosystem compatibility:

* `composer.json` and `composer.lock`
* `vendor/composer/` directory

TODO: a CLI flag or an environment variable to force Shirabe to use compatible paths.


## Error Messages

Error messages, in particular those from PHP built-in functions, are not
intended to be mapped exactly. Plugins or external tools that rely on error
messages may break.


## Version Reporting

Shirabe has its own version, separate from the Composer version it is ported
from. `shirabe --version`, `shirabe about` and `shirabe diagnose` report the
Shirabe version and show the Composer version alongside it.

The pseudo-package `composer` and `composer-runtime-api` keep the original
Composer version in order not to break the version resolution.


## Xdebug

Composer disables Xdebug on startup. Shirabe emulates it, but the exact
behavior is slightly different. See [docs/dev/xdebug.md](./dev/xdebug.md)
for details.


## Signals

Composer runs its abort handler almost immediately after the signal arrives.
Shirabe, however, runs it at the next checkpoint instead, so stopping `shirabe`
command by `Ctrl+C` may take more time than Composer.

Signal handling in plugins and scripts is undefined behavior: it may or may not
work. See [docs/dev/signals.md](./dev/signals.md) for details.


## Plugins

### Reflection

Reflection on Composer's own objects (`$composer`, `$io`, and everything reachable from them) is
not supported. Their properties are invisible to `ReflectionClass::getProperties()` and cannot be
read or written through `ReflectionProperty`; only the public methods reach the real values.
`ReflectionClass::getFileName()` and the method bodies do not describe Composer's sources either.

Reflection on objects a plugin creates itself works as usual.

### Output buffering functions

`ob_*()` functions work as usual in PHP, but cannot capture any output from
Rust side.

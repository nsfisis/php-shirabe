# Known Incompatibilities

NOTE: This is not an exhaustive list. Shirabe is in early development and there are still a number of significant incompatibilities with Composer that are not documented here yet.


## Error Messages

Error messages, in particular those from PHP built-in functions, are not intended to be mapped exactly.
Plugins or external tools that rely on error messages may break.


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

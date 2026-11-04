# Composer runtime bundle

Composer plugins and scripts expect the real PHP classes of Composer itself and
of the packages Composer depends on to be available. Shirabe embeds the
runtime PHP sources into the binary at compile time.

## Building and embedding the bundle

Shirabe archives all PHP sources and resources of Composer and its dependencies,
and embeds the resulting phar file into the binary. Shirabe also calculates a
hash value over all included files and adds a file containing the hash,
`shirabe/bundle-id`, to the archive. This file is used to verify the bundle. See
`crates/shirabe-php-rpc/build.rs` for details.

### Phar signature

A phar signature is an optional signature that verifies the archive's integrity, and
must be appended to the end of the file. Shirabe's runtime bundle has no
signature because the bundle is not always at the end of the executable file.
To disable phar verification, the PHP worker is started with `-d phar.require_hash=0`.
The worker restores it to `1` right after opening the bundle, so that the rest
of the process still verifies the phars it opens. A verification result is
cached per file, and the bundle is never re-verified.

### `__HALT_COMPILER();` tokens

A phar file consists of 3 or 4 sections: a stub, a manifest, the actual contents
and an optional signature. The stub and the manifest are separated by
a `__HALT_COMPILER();` token, which means that Shirabe's executable binary
must not contain the token anywhere except in the phar itself. Note that the
token is case-insensitive.

## Accessing files at runtime

Shirabe tries to open the embedded bundle, and sets the executable as the base
path of autoloading. If loading the phar fails for some reason, e.g. because the
phar extension is not loaded, Shirabe unpacks the archive to `<Shirabe's cache dir>/runtime/<bundle id>` once,
and uses that directory for the base path instead.

NOTE: the environment variable `SHIRABE_COMPOSER_PHP_DIR` can override the
location for development.

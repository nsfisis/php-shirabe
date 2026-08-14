# Composer runtime bundle

Composer plugins and scripts expect the real PHP classes in `Composer\` itself
and packages that Composer depends on to be available. Shirabe embeds the
runtime PHP sources into the binary at compile time.

## Building and embedding the bundle

Shirabe archives all PHP sources and resources of Composer and its dependencies,
and embeds the archived phar file into the binary. Shirabe also calculates a
hash value from all included files and puts a file that contains the hash,
`shirabe/bundle-id`. It is used for verifying the bundle. See
`crates/shirabe-php-rpc/build.rs` for details.

### Phar signature

A phar signature is an optional signature to verify the archive's integrity, and
must be appended to the end of the file. Shirabe's runtime bundle has no
signature because the bundle is not always at the end of the executable file.
To disable phar verification, the PHP worker is started with `-d phar.require_hash=0`.
The worker restores it to `1` right after opening the bundle, so that the rest
of the process still verifies the phars it opens. A verification result is
cached per file, and the bundle is never re-verified.

### `__HALT_COMPILER();` tokens

A phar file consists of 3 or 4 sections: a stub, a manifest, the actual contents
and an optional signature. The stub and the manifest are separated by
`__HALT_COMPILER();` tokens, which means that Shirabe's executable binary
must not contain the tokens except for phar's one. Note that the token is
case-insensitive.

## Accessing files at runtime

Shirabe tries to open the embedded bundle, and sets the executable as the base
path of autoloading. If loading phar fails for some reason, e.g., no phar ext,
Shirabe unpacks the archive to `<Shirabe's cache dir>/runtime/<bundle id>` once,
and uses that directory for the base path instead.

NOTE: the environment variable `SHIRABE_COMPOSER_PHP_DIR` can override the
location for development.

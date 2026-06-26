//! ref: composer/bin/composer

use shirabe_php_shim::{PHP_ENV, PHP_SERVER};

fn main() {
    // Take the $_ENV / $_SERVER snapshots before any putenv() mutates the real environment.
    // See `docs/dev/env-vars-porting.md` for details.
    std::sync::LazyLock::force(&PHP_ENV);
    std::sync::LazyLock::force(&PHP_SERVER);

    let result = shirabe::run(std::env::args().collect());
    let mut exit_code = match result {
        Ok(exit_code) => exit_code,
        Err(e) => {
            eprintln!("{}", e);
            1
        }
    };
    if exit_code > 255 {
        exit_code = 255;
    }
    std::process::exit(exit_code);
}

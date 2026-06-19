//! ref: composer/bin/composer

use shirabe_php_shim::run_shutdown_functions;

fn main() {
    let result = shirabe::run(std::env::args().collect());
    run_shutdown_functions();
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

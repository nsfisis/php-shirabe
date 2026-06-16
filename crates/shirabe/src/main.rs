//! ref: composer/bin/composer

use shirabe::console::Application;
use shirabe::util::Platform;
use shirabe_php_shim::{realpath, run_shutdown_functions};

fn main() {
    // TODO(php-runtime): the full initialization process in composer/bin/composer should be ported
    // somewhere else that communicates with the real PHP runtime.

    Platform::put_env(
        "COMPOSER_BINARY",
        &realpath(&std::env::args().next().unwrap_or_default()).unwrap_or_default(),
    );

    // run the command application
    let application = match Application::new_shared("Composer".to_string(), String::new()) {
        Ok(application) => application,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    let result = Application::run(&application, None, None);
    run_shutdown_functions();
    if let Err(e) = result {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

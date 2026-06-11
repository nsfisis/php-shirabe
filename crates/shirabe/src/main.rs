//! ref: composer/bin/composer

use shirabe::console::Application;
use shirabe::util::Platform;
use shirabe_php_shim::realpath;

fn main() {
    // TODO(php-runtime): the full initialization process in composer/bin/composer should be ported
    // somewhere else that communicates with the real PHP runtime.

    Platform::put_env(
        "COMPOSER_BINARY",
        &realpath(&std::env::args().next().unwrap_or_default()).unwrap_or_default(),
    );

    // run the command application
    let mut application = Application::new("Composer".to_string(), String::new());
    match application.run(None, None) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

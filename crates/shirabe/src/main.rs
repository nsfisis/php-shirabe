//! ref: composer/bin/composer

use shirabe_php_shim::{PHP_ENV, PHP_SERVER};
use std::io::IsTerminal as _;

/// Initialize a tracing subscriber from the environment variable `$SHIRABE_TRACING`.
fn init_tracing() {
    let Ok(directives) = std::env::var("SHIRABE_TRACING") else {
        return;
    };
    if directives.is_empty() {
        return;
    }

    let env_filter = match tracing_subscriber::EnvFilter::builder().parse(&directives) {
        Ok(env_filter) => env_filter,
        Err(e) => {
            eprintln!("SHIRABE_TRACING: invalid filter directives {directives:?}: {e}");
            return;
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();
}

fn main() {
    // Take the $_ENV / $_SERVER snapshots before any putenv() mutates the real environment.
    // See `docs/dev/env-vars-porting.md` for details.
    std::sync::LazyLock::force(&PHP_ENV);
    std::sync::LazyLock::force(&PHP_SERVER);

    // Composer calls XdebugHandler::check() here to restart without Xdebug, which is intentionally
    // unported. See `docs/dev/xdebug.md`.

    // The single process-wide tokio Runtime. `shirabe::run` and everything under it
    // (Command::execute and friends) is still synchronous top to bottom; entering the runtime
    // here (rather than driving `run` via `.block_on`) just makes it ambiently available via
    // `Handle::try_current()` for `util::sync_executor::block_on`'s many scattered call sites,
    // which ride it through `tokio::task::block_in_place` instead of each spinning up (or
    // busy-spin-polling without) their own. See sync_executor.rs for the TODO(phase-e) tracking
    // the eventual goal of propagating `async fn` all the way up to here instead.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build the top-level tokio runtime");
    let _runtime_guard = runtime.enter();

    init_tracing();

    let result = shirabe::run(std::env::args().collect());
    let exit_code = match result {
        Ok(exit_code) => exit_code,
        Err(e) => {
            eprintln!("{}", e);
            1
        }
    };
    std::process::exit(exit_code.min(255));
}

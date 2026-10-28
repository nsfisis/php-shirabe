//! ref: composer/bin/composer

use shirabe_php_shim::{PHP_ENV, PHP_SERVER};
use std::io::IsTerminal as _;

/// Initialize a tracing subscriber from the environment variable `$SHIRABE_TRACING`.
fn init_tracing() {
    let Some(directives) = shirabe_php_shim::getenv("SHIRABE_TRACING") else {
        return;
    };
    let directives = match directives.into_string() {
        Ok(directives) => directives,
        Err(directives) => {
            eprintln!("SHIRABE_TRACING: invalid filter directives {directives:?}: not valid UTF-8");
            return;
        }
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

    // The single process-wide tokio Runtime drives `shirabe::run`, which is `async fn` down to
    // `Command::execute` and the installer. It is a multi-thread runtime because the synchronous
    // APIs left below that chain (`HttpDownloader::get`/`copy`, the RPC handlers serving plugin
    // code) bridge back into it through `util::sync_executor::block_on`, which relies on
    // `tokio::task::block_in_place` and so on this flavor of runtime. The Composer object graph
    // is `!Send`, so the future runs on this thread; the worker threads only serve the reactor
    // and the connection tasks reqwest spawns.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build the top-level tokio runtime");

    init_tracing();

    let result = runtime.block_on(shirabe::run(std::env::args().collect()));
    let exit_code = match result {
        Ok(exit_code) => exit_code,
        Err(e) => {
            eprintln!("{}", e);
            1
        }
    };
    std::process::exit(exit_code.min(255));
}

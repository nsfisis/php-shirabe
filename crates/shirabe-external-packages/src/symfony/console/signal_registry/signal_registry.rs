//! ref: composer/vendor/symfony/console/SignalRegistry/SignalRegistry.php

use indexmap::IndexMap;

/// A signal handler receives the signal number and whether a further handler follows.
pub type SignalHandler = Box<dyn Fn(i64, bool)>;

pub struct SignalRegistry {
    // signal number => list of handlers
    signal_handlers: IndexMap<i64, Vec<SignalHandler>>,
}

impl std::fmt::Debug for SignalRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalRegistry")
            .field("signal_handlers", &self.signal_handlers.keys())
            .finish_non_exhaustive()
    }
}

impl Default for SignalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalRegistry {
    pub fn new() -> Self {
        if shirabe_php_shim::function_exists("pcntl_async_signals") {
            shirabe_php_shim::pcntl_async_signals(true);
        }

        Self {
            signal_handlers: IndexMap::new(),
        }
    }

    pub fn register(&mut self, signal: i64, signal_handler: SignalHandler) {
        if !self.signal_handlers.contains_key(&signal) {
            let previous_callback = shirabe_php_shim::pcntl_signal_get_handler(signal);

            if shirabe_php_shim::is_callable(&previous_callback) {
                // $this->signalHandlers[$signal][] = $previousCallback;
                // The previous handler is an opaque PHP callable obtained from pcntl;
                // it is invoked through the runtime callable mechanism.
                self.signal_handlers
                    .entry(signal)
                    .or_default()
                    .push(Box::new(move |signal, has_next| {
                        shirabe_php_shim::call_php_callable(
                            &previous_callback,
                            &[
                                shirabe_php_shim::PhpMixed::Int(signal),
                                shirabe_php_shim::PhpMixed::Bool(has_next),
                            ],
                        );
                    }));
            }
        }

        self.signal_handlers
            .entry(signal)
            .or_default()
            .push(signal_handler);

        // pcntl_signal($signal, [$this, 'handle'])
        // TODO(plugin): the PHP callback `[$this, 'handle']` captures the registry
        // instance. Wiring this object method as a C-level signal handler requires the
        // runtime callable mechanism; see review notes.
        shirabe_php_shim::pcntl_signal(signal, shirabe_php_shim::PhpMixed::Null);
    }

    pub fn is_supported() -> bool {
        if !shirabe_php_shim::function_exists("pcntl_signal") {
            return false;
        }

        if shirabe_php_shim::explode(
            ",",
            &shirabe_php_shim::ini_get("disable_functions").unwrap_or_default(),
        )
        .contains(&"pcntl_signal".to_string())
        {
            return false;
        }

        true
    }

    pub fn handle(&self, signal: i64) {
        let handlers = match self.signal_handlers.get(&signal) {
            Some(handlers) => handlers,
            None => return,
        };
        let count = handlers.len();

        for (i, signal_handler) in handlers.iter().enumerate() {
            let has_next = i != count - 1;
            signal_handler(signal, has_next);
        }
    }
}

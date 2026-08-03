//! ref: composer/src/Composer/EventDispatcher/EventDispatcher.php

use crate::autoload::ClassLoader;
use crate::composer::PartialComposerHandle;
use crate::composer::PartialComposerWeakHandle;
use crate::dependency_resolver::Transaction;
use crate::dependency_resolver::operation::AnyOperation;
use crate::event_dispatcher::Event;
use crate::event_dispatcher::EventInterface;
use crate::event_dispatcher::EventSubscriberInterface;
use crate::event_dispatcher::ScriptExecutionException;
use crate::event_dispatcher::SubscribedEventEntry;
use crate::installer::BinaryInstaller;
use crate::installer::InstallerEvent;
use crate::installer::PackageEvent;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::plugin::php_plugin_proxy::PluginRpcDispatcher;
use crate::repository::RepositoryInterface;
use crate::script::Event as ScriptEvent;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::symfony::console::output::output_interface;
use shirabe_external_packages::symfony::process::ExecutableFinder;
use shirabe_external_packages::symfony::process::PhpExecutableFinder;
use shirabe_php_rpc::{
    PhpThrow, PluginValue, RustMethodDispatcher, RustObjHandle, call_function,
    call_function_with_dispatcher, call_php_method, call_static_method,
};
use shirabe_php_shim::{
    InvalidArgumentException, PATH_SEPARATOR, PhpMixed, RuntimeException, array_pop, array_push,
    array_search_in_vec, array_splice, file_exists, get_class, hash, implode, ini_get, is_array,
    is_callable, is_object, is_string, krsort, php_regex, preg_quote, realpath,
    spl_autoload_functions, spl_autoload_register, spl_autoload_unregister, spl_object_hash,
    str_contains, str_ends_with, str_replace, str_starts_with, strlen, strpos, strtoupper, substr,
    trim,
};

/// Represents a callable listener. PHP's `callable` may be a string (command, script, or
/// "Class::method"), a `[object|string, method]` pair, or a `\Closure`.
#[derive(Clone)]
pub enum Callable {
    String(String),
    /// `[$className_or_object, $methodName]` array callable. The first element is represented
    /// here as `PhpMixed` to keep parity with PHP's loose typing.
    ///
    /// TODO(plugin): only listeners whose object half lives in the PHP child (`PhpMethod`) are
    /// invocable; an `ArrayCallable` carrying a `PhpMixed` object still has no invocation path.
    ArrayCallable(Box<PhpMixed>, String),
    /// PHP `\Closure`, invoked with the event exactly like `$callable($event)` in
    /// `EventDispatcher::doDispatch`. Today this is only produced by Composer's own commands
    /// registering an inline listener on themselves (e.g. `RequireCommand`'s
    /// `dependencyResolutionCompleted` tracker) — Plugin-supplied closures remain out of scope
    /// pending Plugin API.
    Closure(std::rc::Rc<dyn Fn(&dyn EventInterface) -> PhpMixed>),
    /// `[$subscriber, $methodName]` array callable whose object half is a plugin entity in the
    /// PHP child process, registered through `addSubscriber`. Invoked over RPC.
    PhpMethod(shirabe_php_rpc::PhpObjHandle, String),
}

impl std::fmt::Debug for Callable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Callable::String(s) => f.debug_tuple("String").field(s).finish(),
            Callable::ArrayCallable(first, method) => f
                .debug_tuple("ArrayCallable")
                .field(first)
                .field(method)
                .finish(),
            Callable::Closure(_) => f.write_str("Closure(..)"),
            Callable::PhpMethod(handle, method) => f
                .debug_tuple("PhpMethod")
                .field(handle)
                .field(method)
                .finish(),
        }
    }
}

/// The Event Dispatcher.
///
/// Example in command:
///     `$dispatcher = new EventDispatcher($this->requireComposer(), $this->getApplication()->getIO());`
///     // ...
///     `$dispatcher->dispatch(ScriptEvents::POST_INSTALL_CMD);`
#[derive(Debug)]
pub struct EventDispatcher {
    pub(crate) composer: PartialComposerWeakHandle,
    pub(crate) io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    pub(crate) loader: Option<ClassLoader>,
    pub(crate) process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    pub(crate) listeners: IndexMap<String, IndexMap<i64, Vec<Callable>>>,
    pub(crate) run_scripts: bool,
    event_stack: Vec<String>,
    skip_scripts: Vec<String>,
    previous_hash: Option<String>,
    previous_listeners: IndexMap<String, bool>,
    /// For testing only. Mirrors PHPUnit's `getMockBuilder(EventDispatcher)->onlyMethods(['getListeners'])`:
    /// when set, `get_listeners` returns this closure's result verbatim instead of resolving
    /// registered listeners and package scripts.
    get_listeners_override: Option<GetListenersOverride>,
}

/// For testing only. Holds a closure standing in for an overridden `getListeners` method.
pub struct GetListenersOverride(pub Box<dyn Fn(&dyn EventInterface) -> Vec<Callable>>);

impl std::fmt::Debug for GetListenersOverride {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GetListenersOverride(..)")
    }
}

impl EventDispatcher {
    pub fn new(
        composer: PartialComposerWeakHandle,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) -> Self {
        let process = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                io.clone(),
            ))))
        });
        let event_stack: Vec<String> = Vec::new();
        let skip_scripts_env = Platform::get_env("COMPOSER_SKIP_SCRIPTS").unwrap_or_default();
        let skip_scripts: Vec<String> = skip_scripts_env
            .split(',')
            .map(|v| trim(v, Some(" \t\n\r\0\u{0B}")))
            .filter(|val| !val.is_empty())
            .collect();
        Self {
            composer,
            io,
            loader: None,
            process,
            listeners: IndexMap::new(),
            run_scripts: true,
            event_stack,
            skip_scripts,
            previous_hash: None,
            previous_listeners: IndexMap::new(),
            get_listeners_override: None,
        }
    }

    /// For testing only. Installs a closure that overrides `get_listeners`, mirroring
    /// PHPUnit's `onlyMethods(['getListeners'])->will($this->returnValue(...))` /
    /// `->will($this->returnCallback(...))`.
    pub fn __set_get_listeners_override(
        &mut self,
        callback: Box<dyn Fn(&dyn EventInterface) -> Vec<Callable>>,
    ) {
        self.get_listeners_override = Some(GetListenersOverride(callback));
    }

    /// For testing only. Exposes the protected `getPhpExecCommand`, mirroring the PHP tests'
    /// `new \ReflectionMethod($dispatcher, 'getPhpExecCommand')`.
    pub fn __get_php_exec_command(&self) -> anyhow::Result<String> {
        self.get_php_exec_command()
    }

    /// Set whether script handlers are active or not
    pub fn set_run_scripts(&mut self, run_scripts: bool) {
        self.run_scripts = run_scripts;
    }

    /// Dispatch an event
    pub fn dispatch(
        &mut self,
        event_name: Option<&str>,
        event: Option<&mut dyn EventInterface>,
    ) -> anyhow::Result<i64> {
        match event {
            None => {
                let name = event_name.ok_or_else(|| {
                    anyhow::anyhow!(InvalidArgumentException {
                        message:
                            "If no $event is passed in to Composer\\EventDispatcher\\EventDispatcher::dispatch you have to pass in an $eventName, got null."
                                .to_string(),
                        code: 0,
                    })
                })?;
                let mut event = Event::new(name.to_string(), Vec::new(), IndexMap::new());
                self.do_dispatch(&mut event)
            }
            Some(event) => self.do_dispatch(event),
        }
    }

    /// Dispatch a script event.
    pub fn dispatch_script(
        &mut self,
        event_name: &str,
        dev_mode: bool,
        additional_args: Vec<String>,
        flags: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<i64> {
        let composer = self.composer();
        assert!(
            composer.is_full(),
            "This should only be reached with a fully loaded Composer"
        );

        let event = ScriptEvent::new(
            event_name.to_string(),
            composer.as_full().expect("checked above").downgrade(),
            self.io_clone(),
            dev_mode,
            additional_args,
            flags,
        );
        self.do_dispatch_script(event)
    }

    /// Dispatch a package event.
    pub fn dispatch_package_event(
        &mut self,
        event_name: &str,
        dev_mode: bool,
        local_repo: Box<dyn RepositoryInterface>,
        operations: Vec<AnyOperation>,
        operation: AnyOperation,
    ) -> anyhow::Result<i64> {
        let composer = self.composer();
        assert!(
            composer.is_full(),
            "This should only be reached with a fully loaded Composer"
        );

        let event = PackageEvent::new(
            event_name.to_string(),
            composer.as_full().expect("checked above").downgrade(),
            self.io_clone(),
            dev_mode,
            local_repo,
            operations,
            operation,
        );
        self.do_dispatch_package(event)
    }

    /// Dispatch a installer event.
    pub fn dispatch_installer_event(
        &mut self,
        event_name: &str,
        dev_mode: bool,
        execute_operations: bool,
        transaction: Transaction,
    ) -> anyhow::Result<i64> {
        let composer = self.composer();
        assert!(
            composer.is_full(),
            "This should only be reached with a fully loaded Composer"
        );

        let event = InstallerEvent::new(
            event_name.to_string(),
            composer.as_full().expect("checked above").downgrade(),
            self.io_clone(),
            dev_mode,
            execute_operations,
            transaction,
        );
        self.do_dispatch_installer(event)
    }

    /// Triggers the listeners of an event.
    fn do_dispatch(&mut self, event: &mut dyn EventInterface) -> anyhow::Result<i64> {
        if Platform::get_env("COMPOSER_DEBUG_EVENTS").is_some() {
            // TODO(plugin): PackageEvent / CommandEvent / PreCommandRunEvent specialization
            // requires polymorphic dispatch; the simple Event branch is sufficient for now.
            let details: Option<String> = None;
            self.io.write_error3(
                &format!(
                    "Dispatching <info>{}</info>{} event",
                    event.get_name(),
                    details
                        .as_ref()
                        .map(|d| format!(" ({})", d))
                        .unwrap_or_default()
                ),
                true,
                crate::io::NORMAL,
            );
        }

        let listeners = self.get_listeners(&*event);

        self.push_event(&*event)?;

        let autoloaders_before = spl_autoload_functions();

        let result = self.do_dispatch_body(&*event, listeners);

        // finally block
        self.pop_event();

        let mut known_identifiers: IndexMap<String, IndexMap<String, PhpMixed>> = IndexMap::new();
        for (key, cb) in autoloaders_before.iter().enumerate() {
            let mut entry: IndexMap<String, PhpMixed> = IndexMap::new();
            entry.insert("key".to_string(), PhpMixed::Int(key as i64));
            entry.insert("callback".to_string(), cb.clone());
            known_identifiers.insert(Self::get_callback_identifier(cb), entry);
        }
        for cb in spl_autoload_functions() {
            // once we get to the first known autoloader, we can leave any appended autoloader without problems
            if let Some(entry) = known_identifiers.get(&Self::get_callback_identifier(&cb))
                && entry
                    .get("key")
                    .and_then(|v| v.as_int())
                    .map(|k| k == 0)
                    .unwrap_or(false)
            {
                break;
            }

            // other newly appeared prepended autoloaders should be appended instead to ensure Composer loads its classes first
            // PHP: spl_autoload_unregister($cb); spl_autoload_register($cb, true, $prepend);
            // TODO(plugin): ClassLoader detection via instanceof — currently treat all callbacks uniformly
            // TODO(phase-c): `cb` is a PhpMixed holding a callable; spl_autoload_register/unregister
            // (php-shims that stay todo!()) need a typed Box<dyn Fn(&str) -> PhpMixed> callback.
            // Bridging requires the callable model to expose the underlying closure from PhpMixed.
            let _ = &cb;
            let _ = spl_autoload_unregister;
            let _ = spl_autoload_register;
        }

        result
    }

    fn do_dispatch_body(
        &mut self,
        event: &dyn EventInterface,
        listeners: Vec<Callable>,
    ) -> anyhow::Result<i64> {
        let mut return_max = 0_i64;
        for callable in listeners {
            let mut r#return: i64 = 0;
            self.ensure_bin_dir_is_in_path();

            let mut additional_args = event.get_arguments().clone();
            let mut callable = callable;
            if let Callable::String(ref s) = callable
                && str_contains(s, "@no_additional_args")
            {
                let replaced = Preg::replace(php_regex!("{ ?@no_additional_args}"), "", s);
                callable = Callable::String(replaced);
                additional_args = Vec::new();
            }
            let formatted_event_name_with_args = format!(
                "{}{}",
                event.get_name(),
                if !additional_args.is_empty() {
                    format!(" ({})", additional_args.join(", "))
                } else {
                    "".to_string()
                }
            );
            let is_string_callable = matches!(callable, Callable::String(_));
            if let Callable::Closure(ref closure) = callable {
                self.make_autoloader(event, &callable)?;
                // Closures are always callable in PHP (is_callable() returns true for any \Closure),
                // so the is_callable()/RuntimeException branch below never applies here.
                r#return = if matches!(closure(event), PhpMixed::Bool(false)) {
                    1
                } else {
                    0
                };
            } else if let Callable::PhpMethod(ref handle, ref method_name) = callable {
                self.make_autoloader(event, &callable)?;
                Self::ensure_script_autoloader()?;
                let is_callable_value = unwrap_php_result(call_function_with_dispatcher(
                    "is_callable",
                    vec![PluginValue::List(vec![
                        PluginValue::PhpHandle(handle.clone()),
                        PluginValue::string(method_name.clone()),
                    ])],
                    Some(&mut PluginRpcDispatcher::default()),
                ))?;
                if !matches!(is_callable_value, PluginValue::Bool(true)) {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: format!(
                            "Subscriber {}::{} for event {} is not callable, make sure the function is defined and public",
                            handle.class,
                            method_name,
                            event.get_name()
                        ),
                        code: 0,
                    }));
                }
                self.io.write_error3(
                    &format!(
                        "> {}: {}->{}",
                        formatted_event_name_with_args.clone(),
                        handle.class,
                        method_name,
                    ),
                    true,
                    crate::io::VERBOSE,
                );
                let stub_class = Self::event_stub_class(event).ok_or_else(|| {
                    // TODO(plugin): only the base Event and Script\Event proxy stubs exist so
                    // far; installer/package/plugin events need their own stubs.
                    anyhow::anyhow!(RuntimeException {
                        message: format!(
                            "no proxy stub is available yet for the event `{}` dispatched to {}::{}",
                            event.get_name(),
                            handle.class,
                            method_name,
                        ),
                        code: 0,
                    })
                })?;
                let event_rhandle = shirabe_php_rpc::alloc_rhandle();
                let mut dispatcher = PluginRpcDispatcher {
                    event: Some((event_rhandle, event)),
                };
                let outcome = call_php_method(
                    handle.phandle,
                    method_name,
                    vec![PluginValue::RustHandle(RustObjHandle {
                        rhandle: event_rhandle,
                        class: stub_class.to_string(),
                        epoch: 0,
                        snapshot: None,
                    })],
                    Some(&mut dispatcher),
                )?;
                r#return = match outcome {
                    Ok(value) => {
                        if matches!(value, PluginValue::Bool(false)) {
                            1
                        } else {
                            0
                        }
                    }
                    // TODO(plugin): the original exception class is collapsed to
                    // RuntimeException on this side of the boundary.
                    Err(throw) => {
                        return Err(anyhow::anyhow!(RuntimeException {
                            message: throw.message,
                            code: throw.code,
                        }));
                    }
                };
            } else if !is_string_callable {
                // TODO(plugin): an ArrayCallable whose object half is a PhpMixed has no
                // invocation path; only the is_callable error lane and the verbose echo of the
                // PHP branch are replicated here.
                self.make_autoloader(event, &callable)?;
                if !is_callable(&PhpMixed::Null) {
                    let (class_name, method) = match &callable {
                        Callable::ArrayCallable(first, m) => {
                            let cls = if is_object(first.as_ref()) {
                                get_class(first.as_ref())
                            } else if let PhpMixed::String(s) = first.as_ref() {
                                s.clone()
                            } else {
                                "?".to_string()
                            };
                            (cls, m.clone())
                        }
                        _ => ("?".to_string(), "?".to_string()),
                    };
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: format!(
                            "Subscriber {}::{} for event {} is not callable, make sure the function is defined and public",
                            class_name,
                            method,
                            event.get_name()
                        ),
                        code: 0,
                    }));
                }
                if let Callable::ArrayCallable(first, method_name) = &callable {
                    let prefix = if is_object(first.as_ref()) {
                        get_class(first.as_ref())
                    } else if let PhpMixed::String(s) = first.as_ref() {
                        s.clone()
                    } else {
                        "?".to_string()
                    };
                    self.io.write_error3(
                        &format!(
                            "> {}: {}->{}",
                            formatted_event_name_with_args.clone(),
                            prefix,
                            method_name,
                        ),
                        true,
                        crate::io::VERBOSE,
                    );
                }
                // TODO(plugin): actually invoke callable with $event and inspect result
                r#return = 0;
            } else {
                match callable {
                    Callable::String(ref callable_str) if self.is_composer_script(callable_str) => {
                        self.io.write_error3(
                            &format!(
                                "> {}: {}",
                                formatted_event_name_with_args.clone(),
                                callable_str.clone(),
                            ),
                            true,
                            crate::io::VERBOSE,
                        );

                        let mut script: Vec<String> = substr(callable_str, 1, None)
                            .split(' ')
                            .map(|s| s.to_string())
                            .collect();
                        let script_name = script[0].clone();
                        script.remove(0);

                        let args: Vec<String>;
                        if let Some(index) = array_search_in_vec("@additional_args", &script) {
                            let _ = array_splice::<String>(
                                &mut script,
                                index as i64,
                                Some(0),
                                additional_args.clone(),
                            );
                            args = script.clone();
                        } else {
                            let mut merged = script.clone();
                            merged.extend(additional_args.clone());
                            args = merged;
                        }
                        let mut flags = event.get_flags().clone();
                        if flags.contains_key("script-alias-input") {
                            let args_string = script
                                .iter()
                                .map(|arg| ProcessExecutor::escape(arg))
                                .collect::<Vec<_>>()
                                .join(" ");
                            let existing = flags
                                .get("script-alias-input")
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string();
                            flags.insert(
                                "script-alias-input".to_string(),
                                PhpMixed::String(format!("{} {}", args_string, existing)),
                            );
                        }
                        if strpos(callable_str, "@composer ") == Some(0) {
                            let exec = format!(
                                "{} {} {}",
                                self.get_php_exec_command()?,
                                ProcessExecutor::escape(
                                    &Platform::get_env("COMPOSER_BINARY").unwrap_or_default()
                                ),
                                args.join(" ")
                            );
                            let exit_code = self.execute_tty(&exec)?;
                            if exit_code != 0 {
                                self.io.write_error3(&format!(
                                    "<error>Script {} handling the {} event returned with error code {}</error>",
                                    callable_str.clone(),
                                    event.get_name(),
                                    exit_code
                                ), true, crate::io::QUIET);

                                return Err(anyhow::anyhow!(ScriptExecutionException(
                                    RuntimeException {
                                        message: format!(
                                            "Error Output: {}",
                                            self.process.borrow().get_error_output()
                                        ),
                                        code: exit_code,
                                    }
                                )));
                            }
                        } else {
                            if self
                                .get_listeners(&Event::new(
                                    script_name.clone(),
                                    Vec::new(),
                                    IndexMap::new(),
                                ))
                                .is_empty()
                            {
                                self.io.write_error3(&format!(
                                    "<warning>You made a reference to a non-existent script {}</warning>",
                                    callable_str.clone(),
                                ), true, crate::io::QUIET);
                            }

                            // TODO(plugin): reached only with a fully loaded Composer (script dispatch asserts full upstream).
                            let composer = self.composer();
                            let mut script_event = ScriptEvent::new(
                                script_name.clone(),
                                composer
                                    .as_full()
                                    .expect("script dispatch requires a fully loaded Composer")
                                    .downgrade(),
                                self.io_clone(),
                                // event.isDevMode() is only on InstallerEvent/ScriptEvent/PackageEvent
                                // TODO(plugin): proper dev_mode propagation when polymorphic event is supported
                                false,
                                args,
                                flags,
                            );
                            // TODO(plugin): script_event.set_originating_event(event.clone())
                            match self.dispatch(Some(&script_name), Some(&mut script_event)) {
                                Ok(v) => r#return = v,
                                Err(e) => {
                                    if e.downcast_ref::<ScriptExecutionException>().is_some() {
                                        self.io.write_error3(
                                            &format!(
                                                "<error>Script {} was called via {}</error>",
                                                callable_str.clone(),
                                                event.get_name(),
                                            ),
                                            true,
                                            crate::io::QUIET,
                                        );
                                    }
                                    return Err(e);
                                }
                            }
                        }
                    }
                    Callable::String(ref callable_str) if self.is_php_script(callable_str) => {
                        let pos = strpos(callable_str, "::").unwrap_or(0) as i64;
                        let class_name = substr(callable_str, 0, Some(pos));
                        let method_name = substr(callable_str, pos + 2, None);

                        self.make_autoloader(event, &Callable::String(callable_str.clone()))?;
                        if !self.php_runtime_bool(
                            "class_exists",
                            vec![PluginValue::string(class_name.clone())],
                        )? {
                            self.io.write_error3(&format!(
                                "<warning>Class {} is not autoloadable, can not call {} script</warning>",
                                class_name,
                                event.get_name()
                            ), true, crate::io::QUIET);
                            continue;
                        }
                        if !self.php_runtime_bool(
                            "is_callable",
                            vec![PluginValue::string(callable_str.clone())],
                        )? {
                            self.io.write_error3(&format!(
                                "<warning>Method {} is not callable, can not call {} script</warning>",
                                callable_str,
                                event.get_name()
                            ), true, crate::io::QUIET);
                            continue;
                        }

                        match self.execute_event_php_script(&class_name, &method_name, event) {
                            Ok(v) => {
                                r#return = if let PhpMixed::Bool(false) = v { 1 } else { 0 };
                            }
                            Err(e) => {
                                self.io.write_error3(
                                    &format!(
                                        "<error>Script {} handling the {} event terminated with an exception</error>",
                                        callable_str.clone(),
                                        event.get_name(),
                                    ),
                                    true,
                                    crate::io::QUIET,
                                );
                                return Err(e);
                            }
                        }
                    }
                    Callable::String(ref callable_str) if self.is_command_class(callable_str) => {
                        let class_name = callable_str.clone();

                        self.make_autoloader(
                            event,
                            &Callable::ArrayCallable(
                                Box::new(PhpMixed::String(callable_str.clone())),
                                "run".to_string(),
                            ),
                        )?;
                        // The user's command class extends Symfony's Command, so the child
                        // process needs the real symfony/console classes before it can even
                        // autoload the user class.
                        Self::ensure_composer_php_runtime()?;
                        if !self.php_runtime_bool(
                            "class_exists",
                            vec![PluginValue::string(class_name.clone())],
                        )? {
                            self.io.write_error3(&format!(
                                "<warning>Class {} is not autoloadable, can not call {} script</warning>",
                                class_name,
                                event.get_name()
                            ), true, crate::io::QUIET);
                            continue;
                        }
                        if !self.php_runtime_bool(
                            "is_a",
                            vec![
                                PluginValue::string(class_name.clone()),
                                PluginValue::string(
                                    "Symfony\\Component\\Console\\Command\\Command",
                                ),
                                PluginValue::Bool(true),
                            ],
                        )? {
                            self.io.write_error3(&format!(
                                "<warning>Class {} does not extend Symfony\\Component\\Console\\Command\\Command, can not call {} script</warning>",
                                class_name,
                                event.get_name()
                            ), true, crate::io::QUIET);
                            continue;
                        }
                        if self.php_runtime_bool(
                            "defined",
                            vec![PluginValue::string(format!(
                                "Composer\\Script\\ScriptEvents::{}",
                                str_replace("-", "_", &strtoupper(event.get_name()))
                            ))],
                        )? {
                            self.io.write_error3(&format!(
                                "<warning>You cannot bind {} to a Command class, use a non-reserved name</warning>",
                                event.get_name()
                            ), true, crate::io::QUIET);
                            continue;
                        }

                        // PHP hosts the user's Command class in a throwaway, bare
                        // `Symfony\Component\Console\Application` (NOT Composer's Application),
                        // built by a generated snippet running inside the worker. The command's
                        // output is captured in a BufferedOutput and written back through the
                        // dispatcher's IO; upstream hands the live output object of `$this->io`
                        // to `$app->run()` instead, so only the interleaving with concurrent
                        // writes differs.
                        let args = additional_args
                            .iter()
                            .map(|arg| ProcessExecutor::escape(arg))
                            .collect::<Vec<_>>()
                            .join(" ");
                        let string_input = event
                            .get_flags()
                            .get("script-alias-input")
                            .and_then(|v| v.as_string().map(|s| s.to_string()))
                            .unwrap_or(args);
                        let verbosity = if self.io.is_debug() {
                            output_interface::VERBOSITY_DEBUG
                        } else if self.io.is_very_verbose() {
                            output_interface::VERBOSITY_VERY_VERBOSE
                        } else if self.io.is_verbose() {
                            output_interface::VERBOSITY_VERBOSE
                        } else {
                            output_interface::VERBOSITY_NORMAL
                        };
                        let snippet = format!(
                            r#"
$className = {class_name_lit};
$app = new \Symfony\Component\Console\Application();
$app->setCatchExceptions(false);
if (method_exists($app, 'setCatchErrors')) {{
    $app->setCatchErrors(false);
}}
$app->setAutoExit(false);
$cmd = new $className({event_name_lit});
if (method_exists($app, 'addCommand')) {{
    $app->addCommand($cmd);
}} else {{
    $app->add($cmd);
}}
$app->setDefaultCommand((string) $cmd->getName(), true);
$output = new \Symfony\Component\Console\Output\BufferedOutput({verbosity}, {decorated});
try {{
    $status = $app->run(new \Symfony\Component\Console\Input\StringInput({input_lit}), $output);
    return ['status' => $status, 'output' => $output->fetch()];
}} catch (\Throwable $e) {{
    return ['throw' => [get_class($e), $e->getMessage(), (int) $e->getCode()], 'output' => $output->fetch()];
}}
"#,
                            class_name_lit = php_single_quote(&class_name),
                            event_name_lit = php_single_quote(event.get_name()),
                            input_lit = php_single_quote(&string_input),
                            verbosity = verbosity,
                            decorated = if self.io.is_decorated() {
                                "true"
                            } else {
                                "false"
                            },
                        );
                        Self::ensure_script_autoloader()?;
                        let mut dispatcher = ScriptRpcDispatcher {
                            loader: self.loader.clone(),
                            event: None,
                        };
                        let outcome = call_function_with_dispatcher(
                            "__shirabe_eval",
                            vec![PluginValue::string(snippet)],
                            Some(&mut dispatcher),
                        )?;
                        let result = match outcome {
                            Ok(value) => value.to_php_mixed()?,
                            Err(throw) => {
                                self.io.write_error3(
                                    &format!(
                                        "<error>Script {} handling the {} event terminated with an exception</error>",
                                        callable_str.clone(),
                                        event.get_name(),
                                    ),
                                    true,
                                    crate::io::QUIET,
                                );
                                return Err(anyhow::anyhow!(RuntimeException {
                                    message: throw.message,
                                    code: throw.code,
                                }));
                            }
                        };
                        let command_output = result
                            .as_array()
                            .and_then(|map| map.get("output"))
                            .and_then(|v| v.as_string())
                            .unwrap_or_default()
                            .to_string();
                        if !command_output.is_empty() {
                            self.io.write3(&command_output, false, crate::io::NORMAL);
                        }
                        if let Some(throw) = result.as_array().and_then(|map| map.get("throw")) {
                            let fields = throw
                                .as_list()
                                .expect("the eval snippet reports exceptions as a list");
                            let message = fields
                                .get(1)
                                .and_then(|v| v.as_string())
                                .unwrap_or_default()
                                .to_string();
                            let code = match fields.get(2) {
                                Some(PhpMixed::Int(code)) => *code,
                                _ => 0,
                            };
                            self.io.write_error3(
                                &format!(
                                    "<error>Script {} handling the {} event terminated with an exception</error>",
                                    callable_str.clone(),
                                    event.get_name(),
                                ),
                                true,
                                crate::io::QUIET,
                            );
                            return Err(anyhow::anyhow!(RuntimeException { message, code }));
                        }
                        r#return = match result.as_array().and_then(|map| map.get("status")) {
                            Some(PhpMixed::Int(status)) => *status,
                            other => panic!(
                                "the eval snippet always returns an int status, got {other:?}"
                            ),
                        };
                    }
                    Callable::String(callable_str) => {
                        let args = additional_args
                            .iter()
                            .map(|arg| ProcessExecutor::escape(arg))
                            .collect::<Vec<_>>()
                            .join(" ");

                        // @putenv does not receive arguments
                        let mut exec = if strpos(&callable_str, "@putenv ") == Some(0) {
                            callable_str.clone()
                        } else if str_contains(&callable_str, "@additional_args") {
                            str_replace("@additional_args", &args, &callable_str)
                        } else {
                            format!(
                                "{}{}",
                                callable_str,
                                if args.is_empty() {
                                    "".to_string()
                                } else {
                                    format!(" {}", args)
                                }
                            )
                        };

                        if self.io.is_verbose() {
                            self.io.write_error3(
                                &format!("> {}: {}", event.get_name(), exec.clone()),
                                true,
                                crate::io::NORMAL,
                            );
                        } else if self.event_needs_to_output(event) {
                            self.io.write_error3(
                                &format!("> {}", exec.clone()),
                                true,
                                crate::io::NORMAL,
                            );
                        }

                        let possible_local_binaries = self
                            .composer
                            .upgrade()
                            .expect("Composer was dropped before EventDispatcher use")
                            .borrow_partial()
                            .get_package()
                            .get_binaries();
                        if !possible_local_binaries.is_empty() {
                            for local_exec in &possible_local_binaries {
                                if Preg::is_match(
                                    format!("{{\\b{}$}}", preg_quote(&callable_str, None)),
                                    local_exec,
                                ) {
                                    let caller =
                                        BinaryInstaller::determine_binary_caller(local_exec);
                                    exec = Preg::replace(
                                        format!("{{^{}}}", preg_quote(&callable_str, None)),
                                        &format!("{} {}", caller, local_exec),
                                        &exec,
                                    );
                                    break;
                                }
                            }
                        }

                        if strpos(&exec, "@putenv ") == Some(0) {
                            if strpos(&exec, "=").is_none() {
                                Platform::clear_env(&substr(&exec, 8, None));
                            } else {
                                let after = substr(&exec, 8, None);
                                let parts: Vec<&str> = after.splitn(2, '=').collect();
                                let var = parts[0].to_string();
                                let value = parts[1].to_string();
                                Platform::put_env(&var, &value);
                            }

                            continue;
                        }
                        if strpos(&exec, "@php ") == Some(0) {
                            let mut path_and_args = substr(&exec, 5, None);
                            if Platform::is_windows() {
                                path_and_args = Preg::replace_callback(
                                    php_regex!("{^\\S+}"),
                                    |m| str_replace("/", "\\", &m[0]),
                                    &path_and_args,
                                );
                            }
                            // match somename (not in quote, and not a qualified path) and if it is not a valid path from CWD then try to find it
                            // in $PATH. This allows support for `@php foo` where foo is a binary name found in PATH but not an actual relative path
                            let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                            if Preg::is_match3(
                                php_regex!("{^[^\\'\"\\s/\\\\]+}"),
                                &path_and_args,
                                Some(&mut m),
                            ) {
                                let m0 =
                                    m.get(&CaptureKey::ByIndex(0)).cloned().unwrap_or_default();
                                if !file_exists(&m0) {
                                    let finder = ExecutableFinder::new();
                                    if let Some(path_to_exec) = finder.find(&m0, None, &[]) {
                                        let mut path_to_exec = path_to_exec;
                                        if Platform::is_windows() {
                                            let exec_without_ext = Preg::replace(
                                                php_regex!("{\\.(exe|bat|cmd|com)$}i"),
                                                "",
                                                &path_to_exec,
                                            );
                                            // prefer non-extension file if it exists when executing with PHP
                                            if file_exists(&exec_without_ext) {
                                                path_to_exec = exec_without_ext;
                                            }
                                        }
                                        path_and_args = format!(
                                            "{}{}",
                                            path_to_exec,
                                            substr(&path_and_args, strlen(&m[0]), None)
                                        );
                                    }
                                }
                            }
                            exec = format!("{} {}", self.get_php_exec_command()?, path_and_args);
                        } else {
                            let finder = PhpExecutableFinder::new();
                            let php_path = finder.find(false);
                            if let Some(ref pp) = php_path {
                                Platform::put_env("PHP_BINARY", pp);
                            }

                            if Platform::is_windows() {
                                exec = Preg::replace_callback(
                                    php_regex!("{^\\S+}"),
                                    |m| str_replace("/", "\\", &m[0]),
                                    &exec,
                                );
                            }
                        }

                        // if composer is being executed, make sure it runs the expected composer from current path
                        // resolution, even if bin-dir contains composer too because the project requires composer/composer
                        // see https://github.com/composer/composer/issues/8748
                        if strpos(&exec, "composer ") == Some(0) {
                            exec = format!(
                                "{} {}{}",
                                self.get_php_exec_command()?,
                                ProcessExecutor::escape(
                                    &Platform::get_env("COMPOSER_BINARY").unwrap_or_default()
                                ),
                                substr(&exec, 8, None)
                            );
                        }

                        let exit_code = self.execute_tty(&exec)?;
                        if exit_code != 0 {
                            self.io.write_error3(&format!(
                                "<error>Script {} handling the {} event returned with error code {}</error>",
                                callable_str,
                                event.get_name(),
                                exit_code
                            ), true, crate::io::QUIET);

                            return Err(anyhow::anyhow!(ScriptExecutionException(
                                RuntimeException {
                                    message: format!(
                                        "Error Output: {}",
                                        self.process.borrow().get_error_output()
                                    ),
                                    code: exit_code,
                                }
                            )));
                        }
                    }
                    _ => {
                        // unreachable in practice — the first match arm guard handles non-string callables.
                    }
                }
            }

            return_max = std::cmp::max(return_max, r#return);

            if event.is_propagation_stopped() {
                break;
            }
        }
        Ok(return_max)
    }

    fn do_dispatch_script(&mut self, mut event: ScriptEvent) -> anyhow::Result<i64> {
        self.do_dispatch(&mut event)
    }

    fn do_dispatch_package(&mut self, mut event: PackageEvent) -> anyhow::Result<i64> {
        self.do_dispatch(&mut event)
    }

    fn do_dispatch_installer(&mut self, mut event: InstallerEvent) -> anyhow::Result<i64> {
        self.do_dispatch(&mut event)
    }

    fn execute_tty(&self, exec: &str) -> anyhow::Result<i64> {
        if self.io.is_interactive() {
            return self.process.borrow_mut().execute_tty(exec, None);
        }

        self.process
            .borrow_mut()
            .execute(exec, ProcessExecutor::FORWARD_OUTPUT, None)
    }

    fn get_php_exec_command(&self) -> anyhow::Result<String> {
        let finder = PhpExecutableFinder::new();
        let php_path = finder.find(false);
        let php_path = match php_path {
            Some(p) => p,
            None => {
                return Err(anyhow::anyhow!(RuntimeException {
                    message: "Failed to locate PHP binary to execute ".to_string(),
                    code: 0,
                }));
            }
        };
        let php_args = finder.find_arguments();
        let php_args = if !php_args.is_empty() {
            format!(" {}", implode(" ", &php_args))
        } else {
            "".to_string()
        };
        let allow_url_fopen_flag = format!(
            " -d allow_url_fopen={}",
            ProcessExecutor::escape(&ini_get("allow_url_fopen").unwrap_or_default())
        );
        let disable_functions_flag = format!(
            " -d disable_functions={}",
            ProcessExecutor::escape(&ini_get("disable_functions").unwrap_or_default())
        );
        let memory_limit_flag = format!(
            " -d memory_limit={}",
            ProcessExecutor::escape(&ini_get("memory_limit").unwrap_or_default())
        );

        Ok(format!(
            "{}{}{}{}{}",
            ProcessExecutor::escape(&php_path),
            php_args,
            allow_url_fopen_flag,
            disable_functions_flag,
            memory_limit_flag
        ))
    }

    fn execute_event_php_script(
        &self,
        class_name: &str,
        method_name: &str,
        event: &dyn EventInterface,
    ) -> anyhow::Result<PhpMixed> {
        if self.io.is_verbose() {
            self.io.write_error3(
                &format!("> {}: {}::{}", event.get_name(), class_name, method_name),
                true,
                crate::io::NORMAL,
            );
        } else if self.event_needs_to_output(event) {
            self.io.write_error3(
                &format!("> {}::{}", class_name, method_name),
                true,
                crate::io::NORMAL,
            );
        }

        let stub_class = Self::event_stub_class(event).ok_or_else(|| {
            // TODO(plugin): only the base Event and Script\Event proxy stubs exist so far;
            // installer/package/plugin events need their own stubs.
            anyhow::anyhow!(RuntimeException {
                message: format!(
                    "no proxy stub is available yet for the event `{}` dispatched to {}::{}",
                    event.get_name(),
                    class_name,
                    method_name,
                ),
                code: 0,
            })
        })?;

        Self::ensure_script_autoloader()?;
        let rhandle = shirabe_php_rpc::alloc_rhandle();
        let mut dispatcher = ScriptRpcDispatcher {
            loader: self.loader.clone(),
            event: Some((rhandle, event)),
        };
        let outcome = call_static_method(
            class_name,
            method_name,
            vec![PluginValue::RustHandle(RustObjHandle {
                rhandle,
                class: stub_class.to_string(),
                epoch: 0,
                snapshot: None,
            })],
            Some(&mut dispatcher),
        )?;
        match outcome {
            Ok(value) => Ok(value.to_php_mixed()?),
            // TODO(plugin): the original exception class is collapsed to RuntimeException on
            // this side of the boundary.
            Err(throw) => Err(anyhow::anyhow!(RuntimeException {
                message: throw.message,
                code: throw.code,
            })),
        }
    }

    /// The proxy stub class (same FQCN as the real event class) an event crosses the RPC
    /// boundary as, or `None` when no stub exists for it yet.
    fn event_stub_class(event: &dyn EventInterface) -> Option<&'static str> {
        if event.as_any().downcast_ref::<ScriptEvent>().is_some() {
            Some("Composer\\Script\\Event")
        } else if event.as_any().downcast_ref::<Event>().is_some() {
            Some("Composer\\EventDispatcher\\Event")
        } else {
            None
        }
    }

    fn event_needs_to_output(&self, event: &dyn EventInterface) -> bool {
        // do not output the command being run when using `composer exec` as it is fairly obvious the user is running it
        if event.get_name() == "__exec_command" {
            return false;
        }

        // do not output the command being run when using `composer <script-name>` as it is also fairly obvious the user is running it
        if event
            .get_flags()
            .get("script-alias-input")
            .map(|v| !matches!(v, PhpMixed::Null))
            .unwrap_or(false)
        {
            return false;
        }

        true
    }

    /// Add a listener for a particular event
    pub fn add_listener(&mut self, event_name: &str, listener: Callable, priority: i64) {
        self.listeners
            .entry(event_name.to_string())
            .or_default()
            .entry(priority)
            .or_default()
            .push(listener);
    }

    pub fn remove_listener(&mut self, listener: &Callable) {
        for (_event_name, priorities) in self.listeners.iter_mut() {
            for (_priority, listeners) in priorities.iter_mut() {
                let mut to_remove: Vec<usize> = Vec::new();
                for (index, candidate) in listeners.iter().enumerate() {
                    let same = match (listener, candidate) {
                        (Callable::String(a), Callable::String(b)) => a == b,
                        // TODO(plugin): array callable identity (compare object refs)
                        _ => false,
                    };
                    let array_obj_match = matches!(candidate, Callable::ArrayCallable(_, _))
                        && matches!(listener, Callable::ArrayCallable(_, _));
                    if same || array_obj_match {
                        to_remove.push(index);
                    }
                }
                for idx in to_remove.into_iter().rev() {
                    listeners.remove(idx);
                }
            }
        }
    }

    /// Adds object methods as listeners for the events in getSubscribedEvents
    pub fn add_subscriber(
        &mut self,
        subscriber: &dyn EventSubscriberInterface,
    ) -> anyhow::Result<()> {
        for (event_name, params) in subscriber.get_subscribed_events()? {
            match params {
                SubscribedEventEntry::Method(method) => {
                    self.add_listener(
                        &event_name,
                        Callable::PhpMethod(subscriber.subscriber_handle(), method),
                        0,
                    );
                }
                SubscribedEventEntry::MethodWithPriority(method, priority) => {
                    self.add_listener(
                        &event_name,
                        Callable::PhpMethod(subscriber.subscriber_handle(), method),
                        priority.unwrap_or(0),
                    );
                }
                SubscribedEventEntry::Methods(listeners) => {
                    for (method, priority) in listeners {
                        self.add_listener(
                            &event_name,
                            Callable::PhpMethod(subscriber.subscriber_handle(), method),
                            priority.unwrap_or(0),
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Retrieves all listeners for a given event
    fn get_listeners(&mut self, event: &dyn EventInterface) -> Vec<Callable> {
        // For testing only: a test may override this method, mirroring PHPUnit's
        // `onlyMethods(['getListeners'])`.
        if let Some(override_cb) = &self.get_listeners_override {
            return override_cb.0(event);
        }

        let script_listeners: Vec<Callable> = if self.run_scripts {
            self.get_script_listeners(event)
        } else {
            Vec::new()
        };

        let name = event.get_name().to_string();
        if !self
            .listeners
            .get(&name)
            .map(|m| m.contains_key(&0_i64))
            .unwrap_or(false)
        {
            self.listeners
                .entry(name.clone())
                .or_default()
                .insert(0, Vec::new());
        }
        if let Some(priorities) = self.listeners.get_mut(&name) {
            krsort(priorities);
        }

        let mut listeners = self.listeners.clone();
        if let Some(priorities) = listeners.get_mut(&name)
            && let Some(zero_list) = priorities.get_mut(&0)
        {
            zero_list.extend(script_listeners);
        }

        let mut result: Vec<Callable> = Vec::new();
        if let Some(priorities) = listeners.get(&name) {
            for (_priority, list) in priorities {
                result.extend(list.clone());
            }
        }
        result
    }

    /// Checks if an event has listeners registered
    pub fn has_event_listeners(&mut self, event: &dyn EventInterface) -> bool {
        let listeners = self.get_listeners(event);

        !listeners.is_empty()
    }

    /// Finds all listeners defined as scripts in the package
    fn get_script_listeners(&self, event: &dyn EventInterface) -> Vec<Callable> {
        let composer = self.composer();
        let composer = composer.borrow_partial();
        let package = composer.get_package();
        let scripts = package.get_scripts();

        let event_scripts: Vec<String> = match scripts.get(event.get_name()) {
            Some(v) if !v.is_empty() => v.clone(),
            _ => return Vec::new(),
        };

        if self.skip_scripts.iter().any(|s| s == event.get_name()) {
            self.io.write_error3(
                &format!(
                    "Skipped script listeners for <info>{}</info> because of COMPOSER_SKIP_SCRIPTS",
                    event.get_name()
                ),
                true,
                crate::io::VERBOSE,
            );

            return Vec::new();
        }

        // PHP returns the array of script strings; convert each to Callable::String
        event_scripts.into_iter().map(Callable::String).collect()
    }

    /// Checks if string given references a class path and method
    fn is_php_script(&self, callable: &str) -> bool {
        strpos(callable, " ").is_none() && strpos(callable, "::").is_some()
    }

    /// Checks if string given references a command class
    fn is_command_class(&self, callable: &str) -> bool {
        str_contains(callable, "\\")
            && !str_contains(callable, " ")
            && str_ends_with(callable, "Command")
    }

    /// Checks if string given references a composer run-script
    fn is_composer_script(&self, callable: &str) -> bool {
        str_starts_with(callable, "@")
            && !str_starts_with(callable, "@php ")
            && !str_starts_with(callable, "@putenv ")
    }

    /// Push an event to the stack of active event
    fn push_event(&mut self, event: &dyn EventInterface) -> anyhow::Result<i64> {
        let event_name = event.get_name().to_string();
        if self.event_stack.iter().any(|n| n == &event_name) {
            return Err(anyhow::anyhow!(RuntimeException {
                message: format!(
                    "Circular call to script handler '{}' detected",
                    PhpMixed::String(event_name),
                ),
                code: 0,
            }));
        }

        Ok(array_push(&mut self.event_stack, event_name))
    }

    /// Pops the active event from the stack
    fn pop_event(&mut self) -> Option<String> {
        array_pop(&mut self.event_stack)
    }

    fn ensure_bin_dir_is_in_path(&self) {
        let mut path_env = "PATH";

        // checking if only Path and not PATH is set then we probably need to update the Path env
        // on Windows getenv is case-insensitive so we cannot check it via Platform::getEnv and
        // we need to check in $_SERVER directly
        // TODO(plugin): $_SERVER super-global access not available — approximate via Platform.
        if Platform::get_env(path_env).is_none() && Platform::get_env("Path").is_some() {
            path_env = "Path";
        }

        // add the bin dir to the PATH to make local binaries of deps usable in scripts
        let bin_dir = self
            .composer()
            .borrow_partial()
            .get_config()
            .borrow()
            .get("bin-dir")
            .as_string()
            .map(|s| s.to_string())
            .unwrap_or_default();
        if shirabe_php_shim::is_dir(&bin_dir) {
            let bin_dir = realpath(&bin_dir).unwrap_or(bin_dir);
            let path_value = Platform::get_env(path_env).unwrap_or_default();
            if !Preg::is_match(
                format!(
                    "{{(^|{}){}($|{})}}",
                    PATH_SEPARATOR,
                    preg_quote(&bin_dir, None),
                    PATH_SEPARATOR
                ),
                &path_value,
            ) {
                Platform::put_env(
                    path_env,
                    &format!("{}{}{}", bin_dir, PATH_SEPARATOR, path_value),
                );
            }
        }
    }

    fn get_callback_identifier(cb: &PhpMixed) -> String {
        if let PhpMixed::String(s) = cb {
            return format!("fn:{}", s);
        }
        if is_object(cb) {
            return format!("obj:{}", spl_object_hash(cb));
        }
        if is_array(cb)
            && let PhpMixed::Array(map) = cb
        {
            let entries: Vec<&PhpMixed> = map.values().collect();
            if entries.len() >= 2 {
                let first = entries[0];
                let second = entries[1];
                let prefix = if is_string(first) {
                    if let PhpMixed::String(s) = first {
                        s.clone()
                    } else {
                        "?".to_string()
                    }
                } else {
                    format!("{}#{}", get_class(first), spl_object_hash(first))
                };
                let suffix = if let PhpMixed::String(s) = second {
                    s.clone()
                } else {
                    "?".to_string()
                };
                return format!("array:{}::{}", prefix, suffix);
            }
        }

        // not great but also do not want to break everything here
        "unsupported".to_string()
    }

    fn make_autoloader(
        &mut self,
        event: &dyn EventInterface,
        callable: &Callable,
    ) -> anyhow::Result<()> {
        let composer = self.composer();
        let Some(composer) = composer.as_full() else {
            return Ok(());
        };

        let callable_key = match callable {
            Callable::String(callable_str) => callable_str.clone(),
            Callable::ArrayCallable(first, method) => match first.as_ref() {
                PhpMixed::String(class) => format!("{}::{}", class, method),
                other => format!("{}::{}", get_class(other), method),
            },
            // PHP: get_class($callable[0]).'::'.$callable[1] — the object half's runtime class.
            Callable::PhpMethod(handle, method) => format!("{}::{}", handle.class, method),
            Callable::Closure(_) => "closure".to_string(),
        };
        if self.previous_listeners.contains_key(&callable_key) {
            return Ok(());
        }
        self.previous_listeners.insert(callable_key, true);

        let package = composer.borrow().get_package().clone();
        let repository_manager = composer.borrow().get_repository_manager();
        let local_repository = repository_manager.borrow().get_local_repository();
        let packages = local_repository.get_canonical_packages()?;
        let generator = composer.borrow().get_autoload_generator();
        let mut hash_input = packages
            .iter()
            .map(|p| format!("{}/{}", p.get_name(), p.get_version()))
            .collect::<Vec<_>>()
            .join(",");
        let dev_mode = event
            .as_any()
            .downcast_ref::<ScriptEvent>()
            .map(|e| e.is_dev_mode())
            .or_else(|| {
                event
                    .as_any()
                    .downcast_ref::<PackageEvent>()
                    .map(|e| e.is_dev_mode())
            })
            .or_else(|| {
                event
                    .as_any()
                    .downcast_ref::<InstallerEvent>()
                    .map(|e| e.is_dev_mode())
            });
        if let Some(dev_mode) = dev_mode {
            generator.borrow_mut().set_dev_mode(dev_mode);
            if dev_mode {
                hash_input.push_str("/dev");
            }
        }
        let hash = hash("sha256", &hash_input);

        if self.previous_hash.as_deref() == Some(hash.as_str()) {
            return Ok(());
        }

        self.previous_hash = Some(hash);

        let installation_manager = composer.borrow().get_installation_manager();
        let package_map = generator.borrow().build_package_map(
            &mut *installation_manager.borrow_mut(),
            package.clone(),
            packages,
        )?;
        let map = generator
            .borrow()
            .parse_autoloads(package_map, package, PhpMixed::Bool(false));

        if let Some(loader) = &self.loader {
            loader.unregister();
        }

        let vendor_dir = composer
            .borrow()
            .get_config()
            .borrow()
            .get("vendor-dir")
            .as_string()
            .map(|s| s.to_string());
        let loader = generator.borrow().create_loader(&map, vendor_dir);
        loader.register(false);
        self.loader = Some(loader);
        Ok(())
    }

    /// Makes the worker's script-class autoloader active, so class queries and script execution
    /// in the child can resolve classes through the Rust-side [`ClassLoader`] built by
    /// [`Self::make_autoloader`].
    pub(crate) fn ensure_script_autoloader() -> anyhow::Result<()> {
        unwrap_php_result(call_function(
            "__shirabe_enable_script_autoloader",
            Vec::new(),
        ))?;
        Ok(())
    }

    /// Loads the Composer PHP runtime (symfony/console and friends) into the worker, needed
    /// before a `scripts` Command class can be autoloaded and hosted.
    pub(crate) fn ensure_composer_php_runtime() -> anyhow::Result<()> {
        // TODO(plugin): the real PHP classes are taken from a Composer checkout for now; how
        // they ship with a released Shirabe binary is part of the plugin distribution work.
        let autoload = Self::composer_php_runtime_autoload().ok_or_else(|| {
            anyhow::anyhow!(RuntimeException {
                message: "unable to locate the Composer PHP runtime; set SHIRABE_COMPOSER_PHP_DIR \
                          to a Composer checkout with its vendor directory installed"
                    .to_string(),
                code: 0,
            })
        })?;
        unwrap_php_result(call_function(
            "__shirabe_require",
            vec![PluginValue::string(autoload)],
        ))?;
        Ok(())
    }

    fn composer_php_runtime_autoload() -> Option<String> {
        if let Some(dir) = Platform::get_env("SHIRABE_COMPOSER_PHP_DIR") {
            let path = std::path::Path::new(&dir)
                .join("vendor")
                .join("autoload.php");
            if path.is_file() {
                return path.to_str().map(|s| s.to_string());
            }
        }
        // Development fallback: the Composer checkout sitting next to this workspace.
        let dev = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../composer/vendor/autoload.php");
        if dev.is_file() {
            return dev.canonicalize().ok()?.to_str().map(|s| s.to_string());
        }
        None
    }

    /// Runs a boolean runtime query (`class_exists`, `is_a`, ...) inside the PHP worker, with
    /// the script autoloader active so the query can trigger class loading.
    fn php_runtime_bool(&self, function: &str, args: Vec<PluginValue>) -> anyhow::Result<bool> {
        Self::ensure_script_autoloader()?;
        let mut dispatcher = ScriptRpcDispatcher {
            loader: self.loader.clone(),
            event: None,
        };
        let value = unwrap_php_result(call_function_with_dispatcher(
            function,
            args,
            Some(&mut dispatcher),
        ))?;
        match value {
            PluginValue::Bool(value) => Ok(value),
            other => Err(anyhow::anyhow!(
                "PHP runtime query `{function}` did not return a bool: {other:?}"
            )),
        }
    }

    fn io_clone(&self) -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
        self.io.clone()
    }

    fn composer(&self) -> PartialComposerHandle {
        self.composer
            .upgrade()
            .expect("EventDispatcher must lives longer than Composer")
    }

    fn is_empty_value(value: &PhpMixed) -> bool {
        match value {
            PhpMixed::Null => true,
            PhpMixed::Bool(false) => true,
            PhpMixed::Int(0) => true,
            PhpMixed::Float(f) if *f == 0.0 => true,
            PhpMixed::String(s) => s.is_empty() || s == "0",
            PhpMixed::Array(m) => m.is_empty(),
            PhpMixed::List(l) => l.is_empty(),
            _ => false,
        }
    }
}

/// Serves `CallRustMethod` requests issued by the PHP worker while a script-related call is in
/// flight: Rust handle 0 is the runtime service endpoint (autoload lookups against the
/// Rust-side [`ClassLoader`]), and at most one live event handle is exposed per dispatched
/// call.
///
/// TODO(plugin): this per-call scope stands in for the persistent R table of the plugin
/// activation milestone; a stub retained by the script beyond the call observes an unknown
/// handle error instead of the live object.
struct ScriptRpcDispatcher<'a> {
    loader: Option<ClassLoader>,
    event: Option<(u64, &'a dyn EventInterface)>,
}

impl RustMethodDispatcher for ScriptRpcDispatcher<'_> {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &str,
        args: Vec<PluginValue>,
        _out_param_positions: &[u32],
    ) -> Result<PluginValue, PhpThrow> {
        if rhandle == 0 {
            if method_name == "__shirabe_find_file" {
                let class = match args.first() {
                    Some(PluginValue::String(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
                    other => {
                        return Err(runtime_throw(format!(
                            "__shirabe_find_file expects a class name argument, got {other:?}"
                        )));
                    }
                };
                return Ok(
                    match self
                        .loader
                        .as_mut()
                        .and_then(|loader| loader.find_file(&class))
                    {
                        Some(file) => PluginValue::string(file),
                        None => PluginValue::Null,
                    },
                );
            }
            return Err(runtime_throw(format!(
                "unknown runtime service method `{method_name}`"
            )));
        }
        match self.event {
            Some((event_rhandle, event)) if event_rhandle == rhandle => {
                dispatch_event_method(event, method_name)
            }
            _ => Err(runtime_throw(format!(
                "unknown Rust handle {rhandle} (script-event handles are scoped to a single \
                 dispatched call)"
            ))),
        }
    }
}

/// Serves an event proxy stub's method call, shared by the script dispatcher and the plugin
/// dispatcher (both expose one live event handle per dispatched call).
pub(crate) fn dispatch_event_method(
    event: &dyn EventInterface,
    method_name: &str,
) -> Result<PluginValue, PhpThrow> {
    match method_name {
        "getName" => Ok(PluginValue::string(event.get_name())),
        "getArguments" => Ok(PluginValue::List(
            event
                .get_arguments()
                .iter()
                .map(|arg| PluginValue::string(arg.clone()))
                .collect(),
        )),
        "getFlags" => Ok(PluginValue::from_php_mixed(&PhpMixed::Array(
            event.get_flags().clone(),
        ))),
        "isPropagationStopped" => Ok(PluginValue::Bool(event.is_propagation_stopped())),
        "isDevMode" => match event.as_any().downcast_ref::<ScriptEvent>() {
            Some(script_event) => Ok(PluginValue::Bool(script_event.is_dev_mode())),
            None => Err(runtime_throw(
                "isDevMode is only available on script events".to_string(),
            )),
        },
        // TODO(plugin): getComposer/getIO/stopPropagation and the rest need full proxying
        // of the object graph an event exposes, which does not exist yet.
        other => Err(runtime_throw(format!(
            "the Event method `{other}` is not available over RPC yet"
        ))),
    }
}

fn runtime_throw(message: String) -> PhpThrow {
    PhpThrow {
        exception_class: "RuntimeException".to_string(),
        message,
        code: 0,
    }
}

/// Collapses the two failure lanes of an RPC call into `anyhow`: the callers here treat a PHP
/// exception raised during a runtime query as fatal for the current dispatch.
pub(crate) fn unwrap_php_result(
    outcome: anyhow::Result<Result<PluginValue, PhpThrow>>,
) -> anyhow::Result<PluginValue> {
    match outcome? {
        Ok(value) => Ok(value),
        Err(throw) => Err(anyhow::anyhow!(RuntimeException {
            message: throw.message,
            code: throw.code,
        })),
    }
}

/// Quotes a string as a PHP single-quoted literal for a generated snippet.
fn php_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

// Composer's PartialComposer::setEventDispatcher() accepts any EventDispatcher subclass, so plugins
// may swap in a replacement. The interface captures the methods reached through Composer's accessor
// and through the `Rc<RefCell<dyn EventDispatcherInterface>>` references fed from it.
pub trait EventDispatcherInterface: std::fmt::Debug {
    fn dispatch(
        &mut self,
        event_name: Option<&str>,
        event: Option<&mut dyn EventInterface>,
    ) -> anyhow::Result<i64>;
    fn dispatch_script(
        &mut self,
        event_name: &str,
        dev_mode: bool,
        additional_args: Vec<String>,
        flags: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<i64>;
    fn dispatch_installer_event(
        &mut self,
        event_name: &str,
        dev_mode: bool,
        execute_operations: bool,
        transaction: Transaction,
    ) -> anyhow::Result<i64>;
    fn add_listener(&mut self, event_name: &str, listener: Callable, priority: i64);
    fn add_subscriber(&mut self, subscriber: &dyn EventSubscriberInterface) -> anyhow::Result<()>;
    fn has_event_listeners(&mut self, event: &dyn EventInterface) -> bool;
}

impl EventDispatcherInterface for EventDispatcher {
    fn dispatch(
        &mut self,
        event_name: Option<&str>,
        event: Option<&mut dyn EventInterface>,
    ) -> anyhow::Result<i64> {
        self.dispatch(event_name, event)
    }

    fn dispatch_script(
        &mut self,
        event_name: &str,
        dev_mode: bool,
        additional_args: Vec<String>,
        flags: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<i64> {
        self.dispatch_script(event_name, dev_mode, additional_args, flags)
    }

    fn dispatch_installer_event(
        &mut self,
        event_name: &str,
        dev_mode: bool,
        execute_operations: bool,
        transaction: Transaction,
    ) -> anyhow::Result<i64> {
        self.dispatch_installer_event(event_name, dev_mode, execute_operations, transaction)
    }

    fn add_listener(&mut self, event_name: &str, listener: Callable, priority: i64) {
        self.add_listener(event_name, listener, priority);
    }

    fn add_subscriber(&mut self, subscriber: &dyn EventSubscriberInterface) -> anyhow::Result<()> {
        self.add_subscriber(subscriber)
    }

    fn has_event_listeners(&mut self, event: &dyn EventInterface) -> bool {
        self.has_event_listeners(event)
    }
}

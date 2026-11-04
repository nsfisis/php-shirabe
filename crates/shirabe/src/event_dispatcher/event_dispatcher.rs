//! ref: composer/src/Composer/EventDispatcher/EventDispatcher.php sha1:7ee457208f3d134d389add482cdffa5fce71d424

use crate::autoload::ClassLoader;
use crate::composer::{PartialComposerHandle, PartialComposerWeakHandle};
use crate::dependency_resolver::AnyTransaction;
use crate::dependency_resolver::operation::AnyOperation;
use crate::event_dispatcher::{
    Event, EventFlags, EventInterface, EventSubscriberInterface, ScriptExecutionException,
    SubscribedEventEntry,
};
use crate::installer::{BinaryInstaller, InstallerEvent, PackageEvent};
use crate::io::{IOInterface, IOInterfaceImmutable, IOInterfaceWrite};
use crate::plugin::php_call::PhpCall;
use crate::plugin::post_file_download_event::PostFileDownloadEvent;
use crate::plugin::pre_file_download_event::PreFileDownloadEvent;
use crate::plugin::pre_pool_create_event::PrePoolCreateEvent;
use crate::plugin::rpc::dispatch::{PluginRpcDispatcher, error_throw, unavailable_method};
use crate::plugin::rpc::table::{RustEntity, register_entity, rust_handle_value};
use crate::plugin::throwable_bridge::throw_into_error;
use crate::plugin::{CommandEvent, PreCommandRunEvent};
use crate::repository::InstalledRepositoryInterfaceHandle;
use crate::script::Event as ScriptEvent;
use crate::util::{Platform, ProcessExecutor};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue, RustMethodDispatcher, ToWire};
use shirabe_php_shim::{
    Catch as _, InvalidArgumentException, PhpMixed, RuntimeException, array_pop, array_push,
    array_search_in_vec, array_splice, bconcat, display_to_bytes, error_message, explode,
    explode_with_limit, file_exists, get_class, hash, implode, ini_get, is_callable, is_dir,
    is_object, krsort, os_str_from_bytes, path_from_bytes, path_to_bytes, php_regex, preg_is_match,
    preg_match, preg_quote, preg_replace, preg_replace_callback, realpath, str_replace, strlen,
    strpos, strtoupper, substr, trim2,
};
use shirabe_symfony_console::output::output_interface;
use shirabe_symfony_process::{ExecutableFinder, PhpExecutableFinder};

/// Represents a callable listener. PHP's `callable` may be a string (command, script, or
/// "Class::method"), a `[object|string, method]` pair, or a `\Closure`.
#[derive(Clone)]
pub enum Callable {
    String(BString),
    /// `[$className_or_object, $methodName]` array callable. The first element is represented
    /// here as `PhpMixed` to keep parity with PHP's loose typing.
    ///
    /// TODO(plugin): only listeners whose object half lives in the PHP child (`PhpMethod`) are
    /// invocable; an `ArrayCallable` carrying a `PhpMixed` object still has no invocation path.
    ArrayCallable(Box<PhpMixed>, BString),
    /// PHP `\Closure`, invoked with the event exactly like `$callable($event)` in
    /// `EventDispatcher::doDispatch`. Today this is only produced by Composer's own commands
    /// registering an inline listener on themselves (e.g. `RequireCommand`'s
    /// `dependencyResolutionCompleted` tracker) — Plugin-supplied closures remain out of scope
    /// pending Plugin API.
    Closure(std::rc::Rc<dyn Fn(&dyn EventInterface) -> PhpMixed>),
    /// `[$subscriber, $methodName]` array callable whose object half is a plugin entity in the
    /// PHP child process, registered through `addSubscriber`. Invoked over RPC.
    PhpMethod(shirabe_php_rpc::PhpObjHandle, BString),
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
    composer: PartialComposerWeakHandle,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    loader: Option<ClassLoader>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    listeners: IndexMap<BString, IndexMap<i64, Vec<Callable>>>,
    run_scripts: bool,
    event_stack: Vec<BString>,
    skip_scripts: Vec<BString>,
    previous_hash: Option<BString>,
    previous_listeners: IndexMap<BString, bool>,
    /// For testing only. Mirrors PHPUnit's `getMockBuilder(EventDispatcher)->onlyMethods(['getListeners'])`:
    /// when set, `get_listeners` returns this closure's result verbatim instead of resolving
    /// registered listeners and package scripts.
    get_listeners_override: Option<GetListenersOverride>,
    /// For testing only. Mirrors PHPUnit's `getMockBuilder(EventDispatcher)->onlyMethods(['dispatchScript'])`:
    /// when set, `dispatch_script` returns this closure's result instead of building and
    /// dispatching a script event.
    dispatch_script_override: Option<DispatchScriptOverride>,
}

/// For testing only. Holds a closure standing in for an overridden `getListeners` method.
pub struct GetListenersOverride(pub Box<dyn Fn(&dyn EventInterface) -> Vec<Callable>>);

impl std::fmt::Debug for GetListenersOverride {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GetListenersOverride(..)")
    }
}

/// For testing only. Holds a closure standing in for an overridden `dispatchScript` method.
pub struct DispatchScriptOverride(
    pub Box<dyn Fn(&BStr, bool, &[BString], &EventFlags) -> anyhow::Result<i64>>,
);

impl std::fmt::Debug for DispatchScriptOverride {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DispatchScriptOverride(..)")
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
        let event_stack: Vec<BString> = Vec::new();
        let skip_scripts_env = Platform::get_env("COMPOSER_SKIP_SCRIPTS").unwrap_or_default();
        let skip_scripts: Vec<BString> = explode(",", &skip_scripts_env)
            .iter()
            .map(|v| trim2(v, " \t\n\r\0\u{0B}"))
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
            dispatch_script_override: None,
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

    /// For testing only. Installs a closure that overrides `dispatch_script`, mirroring PHPUnit's
    /// `onlyMethods(['dispatchScript'])->willReturnCallback(...)`.
    pub fn __set_dispatch_script_override(
        &mut self,
        callback: Box<dyn Fn(&BStr, bool, &[BString], &EventFlags) -> anyhow::Result<i64>>,
    ) {
        self.dispatch_script_override = Some(DispatchScriptOverride(callback));
    }

    /// For testing only. Exposes the protected `getPhpExecCommand`, mirroring the PHP tests'
    /// `new \ReflectionMethod($dispatcher, 'getPhpExecCommand')`.
    pub fn __get_php_exec_command(&self) -> anyhow::Result<BString> {
        self.get_php_exec_command()
    }

    /// Set whether script handlers are active or not
    pub fn set_run_scripts(&mut self, run_scripts: bool) {
        self.run_scripts = run_scripts;
    }

    /// Dispatch an event
    pub fn dispatch(
        &mut self,
        event_name: Option<&BStr>,
        event: Option<std::rc::Rc<std::cell::RefCell<dyn EventInterface>>>,
    ) -> anyhow::Result<i64> {
        match event {
            None => {
                let name = event_name.ok_or_else(|| -> anyhow::Error {
                    InvalidArgumentException::new(BString::from("If no $event is passed in to Composer\\EventDispatcher\\EventDispatcher::dispatch you have to pass in an $eventName, got null.")).into()
                })?;
                let event: std::rc::Rc<std::cell::RefCell<dyn EventInterface>> = std::rc::Rc::new(
                    std::cell::RefCell::new(Event::new(name, Vec::new(), EventFlags::default())),
                );
                self.do_dispatch(&event)
            }
            Some(event) => self.do_dispatch(&event),
        }
    }

    /// Dispatch a script event.
    pub fn dispatch_script(
        &mut self,
        event_name: &BStr,
        dev_mode: bool,
        additional_args: Vec<BString>,
        flags: EventFlags,
    ) -> anyhow::Result<i64> {
        if let Some(over) = &self.dispatch_script_override {
            return (over.0)(event_name, dev_mode, &additional_args, &flags);
        }

        let composer = self.composer();
        assert!(
            composer.is_full(),
            "This should only be reached with a fully loaded Composer"
        );

        let event = ScriptEvent::new(
            event_name,
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
        event_name: &BStr,
        dev_mode: bool,
        local_repo: InstalledRepositoryInterfaceHandle,
        operations: Vec<std::rc::Rc<AnyOperation>>,
        operation: std::rc::Rc<AnyOperation>,
    ) -> anyhow::Result<i64> {
        let composer = self.composer();
        assert!(
            composer.is_full(),
            "This should only be reached with a fully loaded Composer"
        );

        let event = PackageEvent::new(
            event_name,
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
        event_name: &BStr,
        dev_mode: bool,
        execute_operations: bool,
        transaction: std::rc::Rc<AnyTransaction>,
    ) -> anyhow::Result<i64> {
        let composer = self.composer();
        assert!(
            composer.is_full(),
            "This should only be reached with a fully loaded Composer"
        );

        let event = InstallerEvent::new(
            event_name,
            composer.as_full().expect("checked above").downgrade(),
            self.io_clone(),
            dev_mode,
            execute_operations,
            transaction,
        );
        self.do_dispatch_installer(event)
    }

    /// Triggers the listeners of an event.
    fn do_dispatch(
        &mut self,
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
    ) -> anyhow::Result<i64> {
        if Platform::get_env("COMPOSER_DEBUG_EVENTS").is_some() {
            // TODO(plugin): PackageEvent / CommandEvent / PreCommandRunEvent specialization
            // requires polymorphic dispatch; the simple Event branch is sufficient for now.
            let details: Option<BString> = None;
            self.io.write_error3(
                bconcat!(
                    "Dispatching <info>",
                    event.borrow().get_name(),
                    "</info>",
                    details
                        .as_ref()
                        .map(|d| bconcat!(" (", d, ")"))
                        .unwrap_or_default(),
                    " event"
                ),
                true,
                crate::io::NORMAL,
            );
        }

        let listeners = self.get_listeners(&*event.borrow());

        self.push_event(&*event.borrow())?;

        let result = self.do_dispatch_body(event, listeners);

        // finally block
        self.pop_event();

        // PHP re-appends the autoloaders a listener prepended so that Composer loads its classes
        // first. Autoloaders only exist in the PHP worker, which re-prepends its own stub
        // autoloader after loading plugin code for the same reason.
        result
    }

    fn do_dispatch_body(
        &mut self,
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        listeners: Vec<Callable>,
    ) -> anyhow::Result<i64> {
        // An event's name, arguments and flags are fixed at construction.
        let event_name = event.borrow().get_name().to_owned();
        let mut return_max = 0_i64;
        for callable in listeners {
            let mut r#return: i64 = 0;
            self.ensure_bin_dir_is_in_path()?;

            let mut additional_args = event.borrow().get_arguments().clone();
            let mut callable = callable;
            if let Callable::String(ref s) = callable
                && s.contains_str("@no_additional_args")
            {
                let replaced = preg_replace(php_regex!("{ ?@no_additional_args}"), "", s);
                callable = Callable::String(replaced);
                additional_args = Vec::new();
            }
            let formatted_event_name_with_args = bconcat!(
                event_name.as_bstr(),
                if !additional_args.is_empty() {
                    bconcat!(" (", implode(", ", &additional_args), ")")
                } else {
                    BString::default()
                }
            );
            let is_string_callable = matches!(callable, Callable::String(_));
            if let Callable::Closure(ref closure) = callable {
                self.make_autoloader(event, &callable)?;
                // Closures are always callable in PHP (is_callable() returns true for any \Closure),
                // so the is_callable()/RuntimeException branch below never applies here.
                r#return = if matches!(closure(&*event.borrow()), PhpMixed::Bool(false)) {
                    1
                } else {
                    0
                };
            } else if let Callable::PhpMethod(ref handle, ref method_name) = callable {
                self.make_autoloader(event, &callable)?;
                Self::ensure_script_autoloader()?;
                let is_callable: bool =
                    PhpCall::Function(BStr::new("is_callable")).run(vec![PluginValue::List(
                        vec![
                            PluginValue::PhpHandle(handle.clone()),
                            PluginValue::String(method_name.clone()),
                        ],
                    )])?;
                if !is_callable {
                    return Err(RuntimeException::new(bconcat!(
                        "Subscriber ",
                        handle.class(),
                        "::",
                        method_name,
                        " for event ",
                        event_name.as_bstr(),
                        " is not callable, make sure the function is defined and public"
                    ))
                    .into());
                }
                self.io.write_error3(
                    bconcat!(
                        "> ",
                        formatted_event_name_with_args,
                        ": ",
                        handle.class(),
                        "->",
                        method_name,
                    ),
                    true,
                    crate::io::VERBOSE,
                );
                let stub_class =
                    Self::event_stub_class(&*event.borrow()).ok_or_else(|| -> anyhow::Error {
                        // TODO(plugin): an event class without a proxy stub cannot cross the boundary.
                        RuntimeException::new(bconcat!(
                            "no proxy stub is available yet for the event `",
                            event_name.as_bstr(),
                            "` dispatched to ",
                            handle.class(),
                            "::",
                            method_name
                        ))
                        .into()
                    })?;
                let value: PluginValue = PhpCall::Method(handle, method_name.as_bstr()).run_with(
                    vec![rust_handle_value(
                        register_entity(RustEntity::Event(event.clone())),
                        stub_class,
                    )],
                    &mut PluginRpcDispatcher,
                )?;
                r#return = if matches!(value, PluginValue::Bool(false)) {
                    1
                } else {
                    0
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
                                BString::from("?")
                            };
                            (cls, m.clone())
                        }
                        _ => (BString::from("?"), BString::from("?")),
                    };
                    return Err(RuntimeException::new(bconcat!(
                        "Subscriber ",
                        class_name,
                        "::",
                        method,
                        " for event ",
                        event_name.as_bstr(),
                        " is not callable, make sure the function is defined and public"
                    ))
                    .into());
                }
                if let Callable::ArrayCallable(first, method_name) = &callable {
                    let prefix = if is_object(first.as_ref()) {
                        get_class(first.as_ref())
                    } else if let PhpMixed::String(s) = first.as_ref() {
                        s.clone()
                    } else {
                        BString::from("?")
                    };
                    self.io.write_error3(
                        bconcat!(
                            "> ",
                            formatted_event_name_with_args,
                            ": ",
                            prefix,
                            "->",
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
                    Callable::String(ref callable_str)
                        if self.is_composer_script(callable_str.as_ref()) =>
                    {
                        self.io.write_error3(
                            bconcat!("> ", formatted_event_name_with_args, ": ", callable_str),
                            true,
                            crate::io::VERBOSE,
                        );

                        let mut script: Vec<BString> = explode(" ", substr(callable_str, 1, None));
                        let script_name = script.remove(0);

                        let args: Vec<BString>;
                        if let Some(index) = array_search_in_vec("@additional_args", &script) {
                            let _ = array_splice::<BString>(
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
                        let mut flags = event.borrow().get_flags().clone();
                        if let Some(script_alias_input) = &mut flags.script_alias_input {
                            let args_string = implode(
                                " ",
                                &script
                                    .iter()
                                    .map(ProcessExecutor::escape)
                                    .collect::<Vec<_>>(),
                            );
                            *script_alias_input =
                                bconcat!(args_string, " ", script_alias_input.as_bstr());
                        }
                        if strpos(callable_str, "@composer ") == Some(0) {
                            // COMPOSER_BINARY is a native executable, so it is run directly instead
                            // of being passed to a PHP interpreter.
                            let exec = bconcat!(
                                ProcessExecutor::escape(
                                    Platform::get_env("COMPOSER_BINARY").unwrap_or_default()
                                ),
                                " ",
                                implode(" ", &args)
                            );
                            let exit_code = self.execute_tty(&exec)?;
                            if exit_code != 0 {
                                self.io.write_error3(
                                    bconcat!(
                                        "<error>Script ",
                                        callable_str,
                                        " handling the ",
                                        event_name.as_bstr(),
                                        " event returned with error code ",
                                        bconcat!(exit_code),
                                        "</error>"
                                    ),
                                    true,
                                    crate::io::QUIET,
                                );

                                return Err(ScriptExecutionException::new(
                                    bconcat!(
                                        "Error Output: ",
                                        self.process.borrow().get_error_output()
                                    ),
                                    exit_code,
                                )
                                .into());
                            }
                        } else {
                            if self
                                .get_listeners(&Event::new(
                                    script_name.clone(),
                                    Vec::new(),
                                    EventFlags::default(),
                                ))
                                .is_empty()
                            {
                                self.io.write_error3(
                                    bconcat!(
                                        "<warning>You made a reference to a non-existent script ",
                                        callable_str,
                                        "</warning>"
                                    ),
                                    true,
                                    crate::io::QUIET,
                                );
                            }

                            // TODO(plugin): reached only with a fully loaded Composer (script dispatch asserts full upstream).
                            let composer = self.composer();
                            let script_event = ScriptEvent::new(
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
                            match self.dispatch(
                                Some(script_name.as_ref()),
                                Some(std::rc::Rc::new(std::cell::RefCell::new(script_event))),
                            ) {
                                Ok(v) => r#return = v,
                                Err(e) => {
                                    if e.is_instanceof::<ScriptExecutionException>() {
                                        self.io.write_error3(
                                            bconcat!(
                                                "<error>Script ",
                                                callable_str,
                                                " was called via ",
                                                event_name.as_bstr(),
                                                "</error>"
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
                    Callable::String(ref callable_str)
                        if self.is_php_script(callable_str.as_ref()) =>
                    {
                        let pos = strpos(callable_str, "::").unwrap_or(0) as i64;
                        let class_name = substr(callable_str, 0, Some(pos));
                        let method_name = substr(callable_str, pos + 2, None);

                        self.make_autoloader(event, &Callable::String(callable_str.clone()))?;
                        if !self.php_runtime_bool(
                            BStr::new("class_exists"),
                            vec![PluginValue::string(class_name.clone())],
                        )? {
                            self.io.write_error3(
                                bconcat!(
                                    "<warning>Class ",
                                    class_name,
                                    " is not autoloadable, can not call ",
                                    event_name.as_bstr(),
                                    " script</warning>"
                                ),
                                true,
                                crate::io::QUIET,
                            );
                            continue;
                        }
                        if !self.php_runtime_bool(
                            BStr::new("is_callable"),
                            vec![PluginValue::string(callable_str.clone())],
                        )? {
                            self.io.write_error3(
                                bconcat!(
                                    "<warning>Method ",
                                    callable_str,
                                    " is not callable, can not call ",
                                    event_name.as_bstr(),
                                    " script</warning>"
                                ),
                                true,
                                crate::io::QUIET,
                            );
                            continue;
                        }

                        match self.execute_event_php_script(
                            class_name.as_ref(),
                            method_name.as_ref(),
                            event,
                        ) {
                            Ok(v) => {
                                r#return = if let PhpMixed::Bool(false) = v { 1 } else { 0 };
                            }
                            Err(e) => {
                                self.io.write_error3(
                                    bconcat!(
                                        "<error>Script ",
                                        callable_str,
                                        " handling the ",
                                        event_name.as_bstr(),
                                        " event terminated with an exception</error>"
                                    ),
                                    true,
                                    crate::io::QUIET,
                                );
                                return Err(e);
                            }
                        }
                    }
                    Callable::String(ref callable_str)
                        if self.is_command_class(callable_str.as_ref()) =>
                    {
                        let class_name = callable_str.clone();

                        self.make_autoloader(
                            event,
                            &Callable::ArrayCallable(
                                Box::new(PhpMixed::string(callable_str.clone())),
                                BString::from("run"),
                            ),
                        )?;
                        // The user's command class extends Symfony's Command, so the child
                        // process needs the real symfony/console classes before it can even
                        // autoload the user class.
                        Self::ensure_composer_php_runtime(|| self.cache_dir())?;
                        if !self.php_runtime_bool(
                            BStr::new("class_exists"),
                            vec![PluginValue::string(class_name.clone())],
                        )? {
                            self.io.write_error3(
                                bconcat!(
                                    "<warning>Class ",
                                    class_name,
                                    " is not autoloadable, can not call ",
                                    event_name.as_bstr(),
                                    " script</warning>"
                                ),
                                true,
                                crate::io::QUIET,
                            );
                            continue;
                        }
                        if !self.php_runtime_bool(
                            BStr::new("is_a"),
                            vec![
                                PluginValue::string(class_name.clone()),
                                PluginValue::string(
                                    "Symfony\\Component\\Console\\Command\\Command",
                                ),
                                PluginValue::Bool(true),
                            ],
                        )? {
                            self.io.write_error3(bconcat!("<warning>Class ", class_name, " does not extend Symfony\\Component\\Console\\Command\\Command, can not call ", event_name.as_bstr(), " script</warning>"), true, crate::io::QUIET);
                            continue;
                        }
                        if self.php_runtime_bool(
                            BStr::new("defined"),
                            vec![PluginValue::string(bconcat!(
                                "Composer\\Script\\ScriptEvents::",
                                str_replace("-", "_", strtoupper(event_name.as_bstr()))
                            ))],
                        )? {
                            self.io.write_error3(
                                bconcat!(
                                    "<warning>You cannot bind ",
                                    event_name.as_bstr(),
                                    " to a Command class, use a non-reserved name</warning>"
                                ),
                                true,
                                crate::io::QUIET,
                            );
                            continue;
                        }

                        // PHP hosts the user's Command class in a throwaway, bare
                        // `Symfony\Component\Console\Application` (NOT Composer's Application),
                        // built by a generated snippet running inside the worker.
                        //
                        // TODO(plugin): the BufferedOutput has to go. The command has to run
                        // against the real output stream, the way upstream hands the live output
                        // object of `$this->io` to `$app->run()`; collecting the output and
                        // writing the buffer back once the run has returned is not a substitute.
                        let args = implode(
                            " ",
                            &additional_args
                                .iter()
                                .map(ProcessExecutor::escape)
                                .collect::<Vec<_>>(),
                        );
                        let string_input = event
                            .borrow()
                            .get_flags()
                            .script_alias_input
                            .clone()
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
                        let snippet = bconcat!(
                            "\n$className = ",
                            php_single_quote(class_name.as_ref()),
                            r#";
$app = new \Symfony\Component\Console\Application();
$app->setCatchExceptions(false);
if (method_exists($app, 'setCatchErrors')) {
    $app->setCatchErrors(false);
}
$app->setAutoExit(false);
$cmd = new $className("#,
                            php_single_quote(event_name.as_bstr()),
                            r#");
if (method_exists($app, 'addCommand')) {
    $app->addCommand($cmd);
} else {
    $app->add($cmd);
}
$app->setDefaultCommand((string) $cmd->getName(), true);
$output = new \Symfony\Component\Console\Output\BufferedOutput("#,
                            verbosity,
                            ", ",
                            if self.io.is_decorated() {
                                "true"
                            } else {
                                "false"
                            },
                            r#");
try {
    $status = $app->run(new \Symfony\Component\Console\Input\StringInput("#,
                            php_single_quote(string_input.as_ref()),
                            r#"), $output);
    return ['status' => $status, 'output' => $output->fetch(), 'throw' => null, 'caught' => false];
} catch (\Throwable $e) {
    return ['status' => null, 'output' => $output->fetch(), 'throw' => \Shirabe\MaterializedThrowable::describe($e), 'caught' => $e instanceof \Exception];
}
"#,
                        );
                        Self::ensure_script_autoloader()?;
                        let mut dispatcher = ScriptRpcDispatcher {
                            loader: self.loader.clone(),
                        };
                        let result: ScriptCommandResult =
                            PhpCall::Function(BStr::new("__shirabe_eval"))
                                .run_with(vec![PluginValue::string(snippet)], &mut dispatcher)?;
                        if !result.output.is_empty() {
                            self.io.write3(&result.output, false, crate::io::NORMAL);
                        }
                        if let Some(throw) = result.throw {
                            if result.caught {
                                self.io.write_error3(
                                    bconcat!(
                                        "<error>Script ",
                                        callable_str,
                                        " handling the ",
                                        event_name.as_bstr(),
                                        " event terminated with an exception</error>"
                                    ),
                                    true,
                                    crate::io::QUIET,
                                );
                            }
                            return Err(throw_into_error(PhpThrow::from_wire(throw)));
                        }
                        r#return = result
                            .status
                            .expect("the snippet reports a status whenever nothing was thrown");
                    }
                    Callable::String(callable_str) => {
                        let args = implode(
                            " ",
                            &additional_args
                                .iter()
                                .map(ProcessExecutor::escape)
                                .collect::<Vec<_>>(),
                        );

                        // @putenv does not receive arguments
                        let mut exec = if strpos(&callable_str, "@putenv ") == Some(0) {
                            callable_str.clone()
                        } else if callable_str.contains_str("@additional_args") {
                            str_replace("@additional_args", &args, &callable_str)
                        } else {
                            bconcat!(
                                callable_str,
                                if args.is_empty() {
                                    BString::default()
                                } else {
                                    bconcat!(" ", args)
                                }
                            )
                        };

                        if self.io.is_verbose() {
                            self.io.write_error3(
                                bconcat!("> ", event_name.as_bstr(), ": ", exec),
                                true,
                                crate::io::NORMAL,
                            );
                        } else if self.event_needs_to_output(&*event.borrow()) {
                            self.io
                                .write_error3(bconcat!("> ", exec), true, crate::io::NORMAL);
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
                                if preg_is_match(
                                    bconcat!("{\\b", preg_quote(&callable_str, None), "$}"),
                                    local_exec,
                                ) {
                                    let caller = BinaryInstaller::determine_binary_caller(
                                        path_from_bytes(local_exec),
                                    );
                                    exec = preg_replace(
                                        bconcat!("{^", preg_quote(&callable_str, None), "}"),
                                        bconcat!(caller, " ", local_exec),
                                        &exec,
                                    );
                                    break;
                                }
                            }
                        }

                        if strpos(&exec, "@putenv ") == Some(0) {
                            if strpos(&exec, "=").is_none() {
                                Platform::clear_env(substr(&exec, 8, None));
                            } else {
                                let parts = explode_with_limit("=", substr(&exec, 8, None), 2);
                                Platform::put_env(&parts[0], &parts[1]);
                            }

                            continue;
                        }
                        if strpos(&exec, "@php ") == Some(0) {
                            let mut path_and_args = substr(&exec, 5, None);
                            if Platform::is_windows() {
                                path_and_args = preg_replace_callback(
                                    php_regex!("{^\\S+}"),
                                    |m| Ok(str_replace("/", "\\", m.get(0).unwrap())),
                                    &path_and_args,
                                )
                                .expect("the replacement callback cannot fail");
                            }
                            // match somename (not in quote, and not a qualified path) and if it is not a valid path from CWD then try to find it
                            // in $PATH. This allows support for `@php foo` where foo is a binary name found in PATH but not an actual relative path
                            if let Some(m) =
                                preg_match(php_regex!("{^[^\\'\"\\s/\\\\]+}"), &path_and_args)
                            {
                                let m0 = m.get(0).unwrap_or_default();
                                if !file_exists(path_from_bytes(m0)) {
                                    let finder = ExecutableFinder::new();
                                    if let Some(path_to_exec) = finder.find(m0, None, &[]) {
                                        let mut path_to_exec = path_to_exec;
                                        if Platform::is_windows() {
                                            let exec_without_ext = preg_replace(
                                                php_regex!("{\\.(exe|bat|cmd|com)$}i"),
                                                "",
                                                &path_to_exec,
                                            );
                                            // prefer non-extension file if it exists when executing with PHP
                                            if file_exists(path_from_bytes(&exec_without_ext)) {
                                                path_to_exec = exec_without_ext;
                                            }
                                        }
                                        path_and_args = bconcat!(
                                            path_to_exec,
                                            substr(&path_and_args, strlen(m0), None)
                                        );
                                    }
                                }
                            }
                            exec = bconcat!(self.get_php_exec_command()?, " ", path_and_args);
                        } else {
                            let finder = PhpExecutableFinder::new();
                            let php_path = finder.find(false);
                            if let Some(ref pp) = php_path {
                                Platform::put_env("PHP_BINARY", pp);
                            }

                            if Platform::is_windows() {
                                exec = preg_replace_callback(
                                    php_regex!("{^\\S+}"),
                                    |m| Ok(str_replace("/", "\\", m.get(0).unwrap())),
                                    &exec,
                                )
                                .expect("the replacement callback cannot fail");
                            }
                        }

                        // if composer is being executed, make sure it runs the expected composer from current path
                        // resolution, even if bin-dir contains composer too because the project requires composer/composer
                        // see https://github.com/composer/composer/issues/8748
                        if strpos(&exec, "composer ") == Some(0) {
                            // COMPOSER_BINARY is a native executable, so it is run directly instead
                            // of being passed to a PHP interpreter.
                            exec = bconcat!(
                                ProcessExecutor::escape(
                                    Platform::get_env("COMPOSER_BINARY").unwrap_or_default()
                                ),
                                substr(&exec, 8, None)
                            );
                        }

                        let exit_code = self.execute_tty(&exec)?;
                        if exit_code != 0 {
                            self.io.write_error3(
                                bconcat!(
                                    "<error>Script ",
                                    callable_str,
                                    " handling the ",
                                    event_name.as_bstr(),
                                    " event returned with error code ",
                                    bconcat!(exit_code),
                                    "</error>"
                                ),
                                true,
                                crate::io::QUIET,
                            );

                            return Err(ScriptExecutionException::new(
                                bconcat!(
                                    "Error Output: ",
                                    self.process.borrow().get_error_output()
                                ),
                                exit_code,
                            )
                            .into());
                        }
                    }
                    _ => {
                        // unreachable in practice — the first match arm guard handles non-string callables.
                    }
                }
            }

            return_max = std::cmp::max(return_max, r#return);

            if event.borrow().is_propagation_stopped() {
                break;
            }
        }
        Ok(return_max)
    }

    fn do_dispatch_script(&mut self, event: ScriptEvent) -> anyhow::Result<i64> {
        self.do_dispatch(
            &(std::rc::Rc::new(std::cell::RefCell::new(event))
                as std::rc::Rc<std::cell::RefCell<dyn EventInterface>>),
        )
    }

    fn do_dispatch_package(&mut self, event: PackageEvent) -> anyhow::Result<i64> {
        self.do_dispatch(
            &(std::rc::Rc::new(std::cell::RefCell::new(event))
                as std::rc::Rc<std::cell::RefCell<dyn EventInterface>>),
        )
    }

    fn do_dispatch_installer(&mut self, event: InstallerEvent) -> anyhow::Result<i64> {
        self.do_dispatch(
            &(std::rc::Rc::new(std::cell::RefCell::new(event))
                as std::rc::Rc<std::cell::RefCell<dyn EventInterface>>),
        )
    }

    fn execute_tty(&self, exec: &BString) -> anyhow::Result<i64> {
        if self.io.is_interactive() {
            return self.process.borrow_mut().execute_tty(exec, None);
        }

        self.process
            .borrow_mut()
            .execute(exec, ProcessExecutor::FORWARD_OUTPUT, None)
    }

    fn get_php_exec_command(&self) -> anyhow::Result<BString> {
        let finder = PhpExecutableFinder::new();
        let php_path = finder.find(false);
        let php_path = match php_path {
            Some(p) => p,
            None => {
                return Err(RuntimeException::new(BString::from(
                    "Failed to locate PHP binary to execute ",
                ))
                .into());
            }
        };
        let php_args = finder.find_arguments();
        let php_args = if !php_args.is_empty() {
            bconcat!(" ", implode(" ", &php_args))
        } else {
            BString::default()
        };
        let allow_url_fopen_flag = bconcat!(
            " -d allow_url_fopen=",
            ProcessExecutor::escape(ini_get("allow_url_fopen").unwrap_or_default())
        );
        let disable_functions_flag = bconcat!(
            " -d disable_functions=",
            ProcessExecutor::escape(ini_get("disable_functions").unwrap_or_default())
        );
        let memory_limit_flag = bconcat!(
            " -d memory_limit=",
            ProcessExecutor::escape(ini_get("memory_limit").unwrap_or_default())
        );

        Ok(bconcat!(
            ProcessExecutor::escape(&php_path),
            php_args,
            allow_url_fopen_flag,
            disable_functions_flag,
            memory_limit_flag
        ))
    }

    fn execute_event_php_script(
        &self,
        class_name: &BStr,
        method_name: &BStr,
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
    ) -> anyhow::Result<PhpMixed> {
        let event_name = event.borrow().get_name().to_owned();
        if self.io.is_verbose() {
            self.io.write_error3(
                bconcat!("> ", event_name, ": ", class_name, "::", method_name),
                true,
                crate::io::NORMAL,
            );
        } else if self.event_needs_to_output(&*event.borrow()) {
            self.io.write_error3(
                bconcat!("> ", class_name, "::", method_name),
                true,
                crate::io::NORMAL,
            );
        }

        let stub_class =
            Self::event_stub_class(&*event.borrow()).ok_or_else(|| -> anyhow::Error {
                // TODO(plugin): an event class without a proxy stub cannot cross the boundary.
                RuntimeException::new(bconcat!(
                    "no proxy stub is available yet for the event `",
                    event_name,
                    "` dispatched to ",
                    class_name,
                    "::",
                    method_name
                ))
                .into()
            })?;

        // The event stub the script receives hands out stubs of the Composer object graph, and
        // those extend and implement the real Composer contracts (`PackageInterface` and the
        // rest), which only the Composer PHP runtime carries.
        Self::ensure_composer_php_runtime(|| self.cache_dir())?;
        Self::ensure_script_autoloader()?;
        let mut dispatcher = ScriptRpcDispatcher {
            loader: self.loader.clone(),
        };
        PhpCall::StaticMethod(class_name, method_name).run_with(
            vec![rust_handle_value(
                register_entity(RustEntity::Event(event.clone())),
                stub_class,
            )],
            &mut dispatcher,
        )
    }

    /// The proxy stub class (same FQCN as the real event class) an event crosses the RPC
    /// boundary as, or `None` when no stub exists for it yet.
    fn event_stub_class(event: &dyn EventInterface) -> Option<&'static BStr> {
        if event.as_any().downcast_ref::<ScriptEvent>().is_some() {
            Some(BStr::new("Composer\\Script\\Event"))
        } else if event.as_any().downcast_ref::<PackageEvent>().is_some() {
            Some(BStr::new("Composer\\Installer\\PackageEvent"))
        } else if event.as_any().downcast_ref::<InstallerEvent>().is_some() {
            Some(BStr::new("Composer\\Installer\\InstallerEvent"))
        } else if event
            .as_any()
            .downcast_ref::<PreFileDownloadEvent>()
            .is_some()
        {
            Some(BStr::new("Composer\\Plugin\\PreFileDownloadEvent"))
        } else if event
            .as_any()
            .downcast_ref::<PostFileDownloadEvent>()
            .is_some()
        {
            Some(BStr::new("Composer\\Plugin\\PostFileDownloadEvent"))
        } else if event
            .as_any()
            .downcast_ref::<PrePoolCreateEvent>()
            .is_some()
        {
            Some(BStr::new("Composer\\Plugin\\PrePoolCreateEvent"))
        } else if event.as_any().downcast_ref::<CommandEvent>().is_some() {
            Some(BStr::new("Composer\\Plugin\\CommandEvent"))
        } else if event
            .as_any()
            .downcast_ref::<PreCommandRunEvent>()
            .is_some()
        {
            Some(BStr::new("Composer\\Plugin\\PreCommandRunEvent"))
        } else if event.as_any().downcast_ref::<Event>().is_some() {
            Some(BStr::new("Composer\\EventDispatcher\\Event"))
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
        if event.get_flags().script_alias_input.is_some() {
            return false;
        }

        true
    }

    /// Add a listener for a particular event
    pub fn add_listener(&mut self, event_name: &BStr, listener: Callable, priority: i64) {
        self.listeners
            .entry(event_name.to_owned())
            .or_default()
            .entry(priority)
            .or_default()
            .push(listener);
    }

    /// PHP's parameter is `callable|object`; every caller in Composer and its test suite
    /// passes an object, and no `Callable` shape holds a bare object, so the parameter is
    /// narrowed to the object's cross-RPC identity (its P-table handle). Of PHP's two match
    /// conditions only `$candidate[0] === $listener` can fire for an object listener; it maps
    /// to handle equality on `Callable::PhpMethod`.
    pub fn remove_listener(&mut self, listener: &shirabe_php_rpc::PhpObjHandle) {
        for priorities in self.listeners.values_mut() {
            for listeners in priorities.values_mut() {
                // TODO(plugin): an `ArrayCallable`'s object half is a `PhpMixed` without
                // cross-RPC identity, so `$candidate[0] === $listener` is undecidable for it;
                // only `PhpMethod` candidates are compared.
                listeners.retain(|candidate| match candidate {
                    Callable::PhpMethod(handle, _) => handle != listener,
                    _ => true,
                });
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
                        event_name.as_ref(),
                        Callable::PhpMethod(subscriber.subscriber_handle(), method),
                        0,
                    );
                }
                SubscribedEventEntry::MethodWithPriority(method, priority) => {
                    self.add_listener(
                        event_name.as_ref(),
                        Callable::PhpMethod(subscriber.subscriber_handle(), method),
                        priority.unwrap_or(0),
                    );
                }
                SubscribedEventEntry::Methods(listeners) => {
                    for (method, priority) in listeners {
                        self.add_listener(
                            event_name.as_ref(),
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

        let name = event.get_name().to_owned();
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

        let event_scripts: Vec<BString> = match scripts.get(event.get_name()) {
            Some(v) if !v.is_empty() => v.clone(),
            _ => return Vec::new(),
        };

        if self.skip_scripts.iter().any(|s| s == event.get_name()) {
            self.io.write_error3(
                bconcat!(
                    "Skipped script listeners for <info>",
                    event.get_name(),
                    "</info> because of COMPOSER_SKIP_SCRIPTS"
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
    fn is_php_script(&self, callable: &BStr) -> bool {
        strpos(callable, " ").is_none() && strpos(callable, "::").is_some()
    }

    /// Checks if string given references a command class
    fn is_command_class(&self, callable: &BStr) -> bool {
        callable.contains_str("\\")
            && !callable.contains_str(" ")
            && callable.ends_with_str("Command")
    }

    /// Checks if string given references a composer run-script
    fn is_composer_script(&self, callable: &BStr) -> bool {
        callable.starts_with_str("@")
            && !callable.starts_with_str("@php ")
            && !callable.starts_with_str("@putenv ")
    }

    /// Push an event to the stack of active event
    fn push_event(&mut self, event: &dyn EventInterface) -> anyhow::Result<i64> {
        let event_name = event.get_name().to_owned();
        if self.event_stack.iter().any(|n| n == &event_name) {
            return Err(RuntimeException::new(bconcat!(
                "Circular call to script handler '",
                event_name,
                "' detected"
            ))
            .into());
        }

        Ok(array_push(&mut self.event_stack, event_name))
    }

    /// Pops the active event from the stack
    fn pop_event(&mut self) -> Option<BString> {
        array_pop(&mut self.event_stack)
    }

    fn ensure_bin_dir_is_in_path(&self) -> anyhow::Result<()> {
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
            .get_path("bin-dir")?;
        let bin_dir = std::path::PathBuf::from(path_from_bytes(&bin_dir));
        if is_dir(&bin_dir) {
            let bin_dir = realpath(&bin_dir).unwrap_or(bin_dir);
            let path_value = Platform::get_env(path_env).unwrap_or_default();
            let path_value = os_str_from_bytes(&path_value);
            if !std::env::split_paths(path_value).any(|dir| dir == bin_dir) {
                let path_value = std::env::join_paths(
                    std::iter::once(bin_dir).chain(std::env::split_paths(path_value)),
                )
                .expect("bin-dir holds no path separator");
                Platform::put_env(path_env, path_to_bytes(&path_value));
            }
        }
        Ok(())
    }

    fn make_autoloader(
        &mut self,
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        callable: &Callable,
    ) -> anyhow::Result<()> {
        let composer = self.composer();
        let Some(composer) = composer.as_full() else {
            return Ok(());
        };

        let callable_key = match callable {
            Callable::String(callable_str) => callable_str.clone(),
            Callable::ArrayCallable(first, method) => match first.as_ref() {
                PhpMixed::String(class) => bconcat!(class, "::", method),
                other => bconcat!(get_class(other), "::", method),
            },
            // PHP: get_class($callable[0]).'::'.$callable[1] — the object half's runtime class.
            Callable::PhpMethod(handle, method) => {
                bconcat!(handle.class(), "::", method)
            }
            Callable::Closure(_) => BString::from("closure"),
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
        let mut hash_input = implode(
            ",",
            &packages
                .iter()
                .map(|p| bconcat!(p.get_name(), "/", p.get_version()))
                .collect::<Vec<_>>(),
        );
        let event = event.borrow();
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
            generator.borrow().set_dev_mode(dev_mode);
            if dev_mode {
                hash_input.extend_from_slice(b"/dev");
            }
        }
        let hash = hash("sha256", &hash_input);

        if self.previous_hash.as_ref() == Some(&hash) {
            return Ok(());
        }

        self.previous_hash = Some(hash);

        let installation_manager = composer.borrow().get_installation_manager();
        let package_map = generator.borrow().build_package_map(
            installation_manager.clone(),
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
            .get_path("vendor-dir")?;
        let loader = generator.borrow().create_loader(&map, Some(vendor_dir))?;
        loader.register(false);
        self.loader = Some(loader);
        Ok(())
    }

    /// Makes the worker's script-class autoloader active, so class queries and script execution
    /// in the child can resolve classes through the Rust-side [`ClassLoader`] built by
    /// [`Self::make_autoloader`].
    pub(crate) fn ensure_script_autoloader() -> anyhow::Result<()> {
        PhpCall::Function(BStr::new("__shirabe_enable_script_autoloader"))
            .run::<PluginValue>(Vec::new())?;
        Ok(())
    }

    /// Loads the Composer PHP runtime (symfony/console and friends) into the worker, needed
    /// before a `scripts` Command class can be autoloaded and hosted.
    pub(crate) fn ensure_composer_php_runtime(
        cache_dir: impl FnOnce() -> anyhow::Result<std::path::PathBuf>,
    ) -> anyhow::Result<()> {
        let autoload = Self::composer_php_runtime_autoload(cache_dir)?;
        PhpCall::Function(BStr::new("__shirabe_require"))
            .run::<PluginValue>(vec![PluginValue::string(autoload)])?;
        crate::repository::FilesystemRepository::load_pending_installed_versions()
    }

    /// For testing only: a test that never registers a plugin package still needs the Composer
    /// PHP runtime in the worker before a class of its own can implement a Composer interface
    /// there.
    pub fn __ensure_composer_php_runtime(cache_dir: &std::path::Path) -> anyhow::Result<()> {
        Self::ensure_composer_php_runtime(|| Ok(cache_dir.to_path_buf()))
    }

    /// The `vendor/autoload.php` of the Composer PHP runtime.
    fn composer_php_runtime_autoload(
        cache_dir: impl FnOnce() -> anyhow::Result<std::path::PathBuf>,
    ) -> anyhow::Result<BString> {
        Ok(bconcat!(
            shirabe_php_rpc::composer_runtime::base_path(cache_dir)?,
            "/vendor/autoload.php"
        ))
    }

    /// Runs a boolean runtime query (`class_exists`, `is_a`, ...) inside the PHP worker, with
    /// the script autoloader active so the query can trigger class loading.
    fn php_runtime_bool(&self, function: &BStr, args: Vec<PluginValue>) -> anyhow::Result<bool> {
        Self::ensure_script_autoloader()?;
        let mut dispatcher = ScriptRpcDispatcher {
            loader: self.loader.clone(),
        };
        PhpCall::Function(function).run_with(args, &mut dispatcher)
    }

    fn cache_dir(&self) -> anyhow::Result<std::path::PathBuf> {
        let cache_dir = self
            .composer()
            .borrow_partial()
            .get_config()
            .borrow()
            .get_path("cache-dir")?;
        Ok(path_from_bytes(&cache_dir).to_path_buf())
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

shirabe_php_rpc::wire_record! {
    /// What the snippet hosting a `scripts` Command class reports about its run: the exit status,
    /// or the exception it ended with and whether Composer would have caught that exception.
    #[derive(Debug)]
    struct ScriptCommandResult {
        status: Option<i64>,
        output: BString,
        throw: Option<PluginValue>,
        caught: bool,
    }
}

/// Serves `CallRustMethod` requests issued by the PHP worker while a script-related call is in
/// flight: Rust handle 0 is the runtime service endpoint (autoload lookups against the
/// Rust-side [`ClassLoader`]), every other rhandle resolves through the R table.
#[derive(Debug)]
struct ScriptRpcDispatcher {
    loader: Option<ClassLoader>,
}

impl RustMethodDispatcher for ScriptRpcDispatcher {
    fn dispatch(
        &mut self,
        rhandle: u64,
        method_name: &BStr,
        args: Vec<PluginValue>,
        out_params: &mut IndexMap<u32, PluginValue>,
    ) -> Result<PluginValue, PhpThrow> {
        let method_name = method_name.to_owned();
        let method_name = method_name.as_bstr();
        if rhandle == 0 {
            if method_name == "__shirabe_find_file" {
                let class = match args.first() {
                    Some(PluginValue::String(bytes)) => bytes,
                    other => {
                        return Err(PhpThrow::runtime(display_to_bytes(format_args!(
                            "__shirabe_find_file expects a class name argument, got {other:?}"
                        ))));
                    }
                };
                let file = match self.loader.as_mut() {
                    Some(loader) => loader
                        .find_file(class)
                        .map_err(|error| error_throw(b"__shirabe_find_file failed", &error))?,
                    None => None,
                };
                return Ok(match file {
                    Some(file) => PluginValue::string(file),
                    None => PluginValue::Null,
                });
            }
            if method_name == "__shirabe_construct" {
                return crate::plugin::rpc::dispatch::construct_entity(&args);
            }
            if method_name == "__shirabe_call_static" {
                return crate::plugin::rpc::dispatch::call_static_entity(&args);
            }
            if let Some(answer) =
                crate::plugin::rpc::dispatch::dispatch_job_service_method(method_name, &args)
            {
                return answer;
            }
            return Err(PhpThrow::runtime(bconcat!(
                "unknown runtime service method `",
                method_name,
                "`"
            )));
        }
        crate::plugin::rpc::dispatch::dispatch_r_table_method(
            rhandle,
            method_name,
            &args,
            out_params,
        )
    }
}

/// Serves a method call on an event entity of the R table.
pub(crate) fn dispatch_event_method(
    event: &mut dyn EventInterface,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getName" => Ok(PluginValue::string(event.get_name())),
        b"getArguments" => Ok(PluginValue::List(
            event
                .get_arguments()
                .iter()
                .map(|arg| PluginValue::string(arg.clone()))
                .collect(),
        )),
        b"getFlags" => Ok(PhpMixed::Array(event.get_flags().to_php_array()).to_wire()),
        b"isPropagationStopped" => Ok(PluginValue::Bool(event.is_propagation_stopped())),
        b"stopPropagation" => {
            event.stop_propagation();
            Ok(PluginValue::Null)
        }
        b"isDevMode" => match (
            script_event(event),
            package_event(event),
            installer_event(event),
        ) {
            (Some(event), _, _) => Ok(PluginValue::Bool(event.is_dev_mode())),
            (_, Some(event), _) => Ok(PluginValue::Bool(event.is_dev_mode())),
            (_, _, Some(event)) => Ok(PluginValue::Bool(event.is_dev_mode())),
            _ => Err(PhpThrow::runtime(BString::from(
                "isDevMode is only available on script, package and installer events",
            ))),
        },
        b"getComposer" => {
            let composer = match (
                script_event(event),
                package_event(event),
                installer_event(event),
            ) {
                (Some(event), _, _) => event.get_composer().upgrade(),
                (_, Some(event), _) => event.get_composer().upgrade(),
                (_, _, Some(event)) => event.get_composer().upgrade(),
                _ => {
                    return Err(PhpThrow::runtime(BString::from(
                        "getComposer is only available on script, package and installer events",
                    )));
                }
            };
            let composer = composer.ok_or_else(|| {
                PhpThrow::runtime(BString::from("the Composer instance of this event is gone"))
            })?;
            let rhandle = crate::plugin::rpc::table::register_composer_entity(&composer);
            Ok(crate::plugin::rpc::table::rust_handle_value(
                rhandle,
                BStr::new("Composer\\Composer"),
            ))
        }
        b"getIO" => {
            let io = match (
                script_event(event),
                package_event(event),
                installer_event(event),
            ) {
                (Some(event), _, _) => event.get_io(),
                (_, Some(event), _) => event.get_io(),
                (_, _, Some(event)) => event.get_io(),
                _ => {
                    return Err(PhpThrow::runtime(BString::from(
                        "getIO is only available on script, package and installer events",
                    )));
                }
            };
            let class = crate::plugin::rpc::table::io_stub_class(&io)
                .map_err(|error| PhpThrow::runtime(error_message(&error)))?;
            let rhandle = crate::plugin::rpc::table::register_io_entity(&io);
            Ok(crate::plugin::rpc::table::rust_handle_value(rhandle, class))
        }
        b"getLocalRepo" => match package_event(event) {
            Some(event) => crate::plugin::rpc::table::repository_handle_value(
                &event.get_local_repo().as_repository_handle(),
            ),
            None => Err(PhpThrow::runtime(BString::from(
                "getLocalRepo is only available on package events",
            ))),
        },
        b"getOperations" => match package_event(event) {
            Some(event) => Ok(PluginValue::List(
                event
                    .get_operations()
                    .iter()
                    .map(crate::plugin::rpc::table::operation_handle_value)
                    .collect(),
            )),
            None => Err(PhpThrow::runtime(BString::from(
                "getOperations is only available on package events",
            ))),
        },
        b"getOperation" => match package_event(event) {
            Some(event) => Ok(crate::plugin::rpc::table::operation_handle_value(
                event.get_operation(),
            )),
            None => Err(PhpThrow::runtime(BString::from(
                "getOperation is only available on package events",
            ))),
        },
        b"getCommandName" => match event.as_any().downcast_ref::<CommandEvent>() {
            Some(event) => Ok(PluginValue::string(event.get_command_name())),
            None => Err(PhpThrow::runtime(BString::from(
                "getCommandName is only available on command events",
            ))),
        },
        b"getCommand" => match event.as_any().downcast_ref::<PreCommandRunEvent>() {
            Some(event) => Ok(PluginValue::string(event.get_command())),
            None => Err(PhpThrow::runtime(BString::from(
                "getCommand is only available on pre-command-run events",
            ))),
        },
        // TODO(plugin): symfony/console lives in both worlds independently, and nothing hands
        // the Rust-side console input and output to the plugin process yet.
        b"getInput" | b"getOutput"
            if event.as_any().downcast_ref::<CommandEvent>().is_some()
                || event
                    .as_any()
                    .downcast_ref::<PreCommandRunEvent>()
                    .is_some() =>
        {
            Err(PhpThrow::runtime(bconcat!(
                "the console ",
                match method_name.as_bytes() {
                    b"getInput" => "input",
                    b"getOutput" => "output",
                    _ => unreachable!(),
                },
                " of a command event is not available over RPC yet"
            )))
        }
        b"isExecutingOperations" => match installer_event(event) {
            Some(event) => Ok(PluginValue::Bool(event.is_executing_operations())),
            None => Err(PhpThrow::runtime(BString::from(
                "isExecutingOperations is only available on installer events",
            ))),
        },
        b"getTransaction" => match installer_event(event) {
            Some(event) => match event.get_transaction() {
                Some(transaction) => Ok(crate::plugin::rpc::table::transaction_handle_value(
                    transaction,
                )),
                None => Ok(PluginValue::Null),
            },
            None => Err(PhpThrow::runtime(BString::from(
                "getTransaction is only available on installer events",
            ))),
        },
        other => {
            if let Some(event) = event.as_any().downcast_ref::<PostFileDownloadEvent>() {
                return crate::plugin::rpc::entities::events::dispatch_post_file_download_event_method(
                    event,
                    BStr::new(other),
                );
            }
            if let Some(event) = event.as_any_mut().downcast_mut::<PrePoolCreateEvent>() {
                return crate::plugin::rpc::entities::events::dispatch_pre_pool_create_event_method(
                    event,
                    BStr::new(other),
                    args,
                );
            }
            match event.as_any_mut().downcast_mut::<PreFileDownloadEvent>() {
                Some(event) => {
                    crate::plugin::rpc::entities::events::dispatch_pre_file_download_event_method(
                        event,
                        BStr::new(other),
                        args,
                    )
                }
                // TODO(plugin): the rest need full proxying of the object graph an event exposes,
                // which does not exist yet.
                None => Err(unavailable_method("Event", other)),
            }
        }
    }
}

fn script_event(event: &dyn EventInterface) -> Option<&ScriptEvent> {
    event.as_any().downcast_ref::<ScriptEvent>()
}

fn package_event(event: &dyn EventInterface) -> Option<&PackageEvent> {
    event.as_any().downcast_ref::<PackageEvent>()
}

fn installer_event(event: &dyn EventInterface) -> Option<&InstallerEvent> {
    event.as_any().downcast_ref::<InstallerEvent>()
}

/// Quotes a string as a PHP single-quoted literal for a generated snippet.
fn php_single_quote(value: &BStr) -> BString {
    bconcat!("'", value.replace("\\", "\\\\").replace("'", "\\'"), "'")
}

// Composer's PartialComposer::setEventDispatcher() accepts any EventDispatcher subclass, so plugins
// may swap in a replacement. The interface captures the methods reached through Composer's accessor
// and through the `Rc<RefCell<dyn EventDispatcherInterface>>` references fed from it.
pub trait EventDispatcherInterface: std::fmt::Debug {
    fn dispatch(
        &mut self,
        event_name: Option<&BStr>,
        event: Option<std::rc::Rc<std::cell::RefCell<dyn EventInterface>>>,
    ) -> anyhow::Result<i64>;
    fn dispatch_script(
        &mut self,
        event_name: &BStr,
        dev_mode: bool,
        additional_args: Vec<BString>,
        flags: EventFlags,
    ) -> anyhow::Result<i64>;
    fn dispatch_installer_event(
        &mut self,
        event_name: &BStr,
        dev_mode: bool,
        execute_operations: bool,
        transaction: std::rc::Rc<AnyTransaction>,
    ) -> anyhow::Result<i64>;
    fn add_listener(&mut self, event_name: &BStr, listener: Callable, priority: i64);
    fn add_subscriber(&mut self, subscriber: &dyn EventSubscriberInterface) -> anyhow::Result<()>;
    fn remove_listener(&mut self, listener: &shirabe_php_rpc::PhpObjHandle);
    fn has_event_listeners(&mut self, event: &dyn EventInterface) -> bool;
}

impl EventDispatcherInterface for EventDispatcher {
    fn dispatch(
        &mut self,
        event_name: Option<&BStr>,
        event: Option<std::rc::Rc<std::cell::RefCell<dyn EventInterface>>>,
    ) -> anyhow::Result<i64> {
        self.dispatch(event_name, event)
    }

    fn dispatch_script(
        &mut self,
        event_name: &BStr,
        dev_mode: bool,
        additional_args: Vec<BString>,
        flags: EventFlags,
    ) -> anyhow::Result<i64> {
        self.dispatch_script(event_name, dev_mode, additional_args, flags)
    }

    fn dispatch_installer_event(
        &mut self,
        event_name: &BStr,
        dev_mode: bool,
        execute_operations: bool,
        transaction: std::rc::Rc<AnyTransaction>,
    ) -> anyhow::Result<i64> {
        self.dispatch_installer_event(event_name, dev_mode, execute_operations, transaction)
    }

    fn add_listener(&mut self, event_name: &BStr, listener: Callable, priority: i64) {
        self.add_listener(event_name, listener, priority);
    }

    fn add_subscriber(&mut self, subscriber: &dyn EventSubscriberInterface) -> anyhow::Result<()> {
        self.add_subscriber(subscriber)
    }

    fn remove_listener(&mut self, listener: &shirabe_php_rpc::PhpObjHandle) {
        self.remove_listener(listener);
    }

    fn has_event_listeners(&mut self, event: &dyn EventInterface) -> bool {
        self.has_event_listeners(event)
    }
}

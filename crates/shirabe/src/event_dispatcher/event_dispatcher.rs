//! ref: composer/src/Composer/EventDispatcher/EventDispatcher.php

use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::console::application::Application;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::component::console::input::string_input::StringInput;
use shirabe_external_packages::symfony::component::console::output::console_output::ConsoleOutput;
use shirabe_external_packages::symfony::component::process::executable_finder::ExecutableFinder;
use shirabe_external_packages::symfony::component::process::php_executable_finder::PhpExecutableFinder;
use shirabe_php_shim::{
    array_pop, array_push, array_search_in_vec, array_splice, class_exists, count_mixed,
    defined, file_exists, get_class, hash, implode, ini_get, is_a, is_array, is_callable,
    is_object, is_string, krsort, max_i64, method_exists, preg_quote, realpath, spl_autoload_functions,
    spl_autoload_register, spl_autoload_unregister, spl_object_hash, sprintf, str_contains,
    str_ends_with, str_replace, str_starts_with, strlen, strpos, strtoupper, substr, trim,
    Exception, InvalidArgumentException, LogicException, PhpMixed, RuntimeException, PATH_SEPARATOR,
    PHP_VERSION_ID,
};

use crate::autoload::class_loader::ClassLoader;
use crate::composer::Composer;
use crate::dependency_resolver::operation::operation_interface::OperationInterface;
use crate::dependency_resolver::transaction::Transaction;
use crate::event_dispatcher::event::Event;
use crate::event_dispatcher::event_subscriber_interface::EventSubscriberInterface;
use crate::event_dispatcher::script_execution_exception::ScriptExecutionException;
use crate::installer::binary_installer::BinaryInstaller;
use crate::installer::installer_event::InstallerEvent;
use crate::installer::package_event::PackageEvent;
use crate::io::console_io::ConsoleIO;
use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::partial_composer::PartialComposer;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::pre_command_run_event::PreCommandRunEvent;
use crate::repository::repository_interface::RepositoryInterface;
use crate::script::event::Event as ScriptEvent;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;

/// Represents a callable listener. PHP's `callable` may be a string (command, script, or
/// "Class::method"), a `[object|string, method]` pair, or a `\Closure`.
///
/// TODO(plugin): Subscriber- and Closure-based listeners come from plugins and are not
/// implemented yet — only the string forms used by composer.json `scripts` work here.
#[derive(Debug, Clone)]
pub enum Callable {
    String(String),
    /// `[$className_or_object, $methodName]` array callable. The first element is represented
    /// here as `PhpMixed` to keep parity with PHP's loose typing.
    ArrayCallable(Box<PhpMixed>, String),
    /// PHP `\Closure` placeholder.
    Closure,
}

/// The Event Dispatcher.
///
/// Example in command:
///     `$dispatcher = new EventDispatcher($this->requireComposer(), $this->getApplication()->getIO());`
///     // ...
///     `$dispatcher->dispatch(ScriptEvents::POST_INSTALL_CMD);`
#[derive(Debug)]
pub struct EventDispatcher {
    pub(crate) composer: PartialComposer,
    pub(crate) io: Box<dyn IOInterface>,
    pub(crate) loader: Option<ClassLoader>,
    pub(crate) process: ProcessExecutor,
    pub(crate) listeners: IndexMap<String, IndexMap<i64, Vec<Callable>>>,
    pub(crate) run_scripts: bool,
    event_stack: Vec<String>,
    skip_scripts: Vec<String>,
    previous_hash: Option<String>,
    previous_listeners: IndexMap<String, bool>,
}

impl EventDispatcher {
    pub fn new(
        composer: PartialComposer,
        io: Box<dyn IOInterface>,
        process: Option<ProcessExecutor>,
    ) -> Self {
        let process = process.unwrap_or_else(|| ProcessExecutor::new(&*io));
        let event_stack: Vec<String> = Vec::new();
        let skip_scripts_env =
            Platform::get_env("COMPOSER_SKIP_SCRIPTS").unwrap_or_else(|| "".to_string());
        let skip_scripts: Vec<String> = skip_scripts_env
            .split(',')
            .map(|v| trim(v, " \t\n\r\0\u{0B}"))
            .filter(|val| val != "")
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
        }
    }

    /// Set whether script handlers are active or not
    pub fn set_run_scripts(&mut self, run_scripts: bool) -> &mut Self {
        self.run_scripts = run_scripts;

        self
    }

    /// Dispatch an event
    pub fn dispatch(
        &mut self,
        event_name: Option<&str>,
        event: Option<Event>,
    ) -> anyhow::Result<i64> {
        let event = match event {
            None => {
                let name = event_name.ok_or_else(|| {
                    anyhow::anyhow!(InvalidArgumentException {
                        message:
                            "If no $event is passed in to Composer\\EventDispatcher\\EventDispatcher::dispatch you have to pass in an $eventName, got null."
                                .to_string(),
                        code: 0,
                    })
                })?;
                Event::new(name.to_string(), Vec::new(), IndexMap::new())
            }
            Some(e) => e,
        };

        self.do_dispatch(event)
    }

    /// Dispatch a script event.
    pub fn dispatch_script(
        &mut self,
        event_name: &str,
        dev_mode: bool,
        additional_args: Vec<String>,
        flags: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<i64> {
        let composer = self.composer_as_full_or_panic();
        let event = ScriptEvent::new(
            event_name.to_string(),
            composer,
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
        operations: Vec<Box<dyn OperationInterface>>,
        operation: Box<dyn OperationInterface>,
    ) -> anyhow::Result<i64> {
        let composer = self.composer_as_full_or_panic();
        let event = PackageEvent::new(
            event_name.to_string(),
            composer,
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
        let composer = self.composer_as_full_or_panic();
        let event = InstallerEvent::new(
            event_name.to_string(),
            composer,
            self.io_clone(),
            dev_mode,
            execute_operations,
            transaction,
        );
        self.do_dispatch_installer(event)
    }

    /// Triggers the listeners of an event.
    fn do_dispatch(&mut self, event: Event) -> anyhow::Result<i64> {
        if Platform::get_env("COMPOSER_DEBUG_EVENTS").is_some() {
            // TODO(plugin): PackageEvent / CommandEvent / PreCommandRunEvent specialization
            // requires polymorphic dispatch; the simple Event branch is sufficient for now.
            let details: Option<String> = None;
            self.io.write_error(
                PhpMixed::String(format!(
                    "Dispatching <info>{}</info>{} event",
                    event.get_name(),
                    details
                        .as_ref()
                        .map(|d| format!(" ({})", d))
                        .unwrap_or_default()
                )),
                true,
                <dyn IOInterface>::NORMAL,
            );
        }

        let listeners = self.get_listeners(&event);

        self.push_event(&event)?;

        let autoloaders_before = spl_autoload_functions();

        let result = self.do_dispatch_body(&event, listeners);

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
            if let Some(entry) = known_identifiers.get(&Self::get_callback_identifier(&cb)) {
                if entry
                    .get("key")
                    .and_then(|v| v.as_int())
                    .map(|k| k == 0)
                    .unwrap_or(false)
                {
                    break;
                }
            }

            // other newly appeared prepended autoloaders should be appended instead to ensure Composer loads its classes first
            // TODO(plugin): ClassLoader detection via instanceof — currently treat all callbacks uniformly
            spl_autoload_unregister(cb.clone());
            spl_autoload_register(cb);
        }

        result
    }

    fn do_dispatch_body(
        &mut self,
        event: &Event,
        listeners: Vec<Callable>,
    ) -> anyhow::Result<i64> {
        let mut return_max = 0_i64;
        for callable in listeners {
            let mut r#return: i64 = 0;
            self.ensure_bin_dir_is_in_path();

            let mut additional_args = event.get_arguments().clone();
            let mut callable = callable;
            if let Callable::String(ref s) = callable {
                if str_contains(s, "@no_additional_args") {
                    let replaced =
                        Preg::replace("{ ?@no_additional_args}", "", s).unwrap_or_else(|_| s.clone());
                    callable = Callable::String(replaced);
                    additional_args = Vec::new();
                }
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
            if !is_string_callable {
                // TODO(plugin): non-string callable handling — verify is_callable, invoke,
                // and replicate the get_class / write_error / is_callable error path from PHP.
                self.make_autoloader(event, &callable);
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
                    self.io.write_error(
                        PhpMixed::String(sprintf(
                            "> %s: %s",
                            &[
                                PhpMixed::String(formatted_event_name_with_args.clone()),
                                PhpMixed::String(format!("{}->{}", prefix, method_name)),
                            ],
                        )),
                        true,
                        <dyn IOInterface>::VERBOSE,
                    );
                }
                // TODO(plugin): actually invoke callable with $event and inspect result
                r#return = 0;
            } else { match callable {
                Callable::String(ref callable_str) if self.is_composer_script(callable_str) => {
                    self.io.write_error(
                        PhpMixed::String(sprintf(
                            "> %s: %s",
                            &[
                                PhpMixed::String(formatted_event_name_with_args.clone()),
                                PhpMixed::String(callable_str.clone()),
                            ],
                        )),
                        true,
                        <dyn IOInterface>::VERBOSE,
                    );

                    let mut script: Vec<String> = substr(callable_str, 1, None)
                        .split(' ')
                        .map(|s| s.to_string())
                        .collect();
                    let script_name = script[0].clone();
                    script.remove(0);

                    let args: Vec<String>;
                    if let Some(index) = array_search_in_vec("@additional_args", &script) {
                        let _ = array_splice::<String>(&mut script, index, 0, &additional_args);
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
                            self.io.write_error(
                                PhpMixed::String(sprintf(
                                    &format!(
                                        "<error>Script %s handling the %s event returned with error code {}</error>",
                                        exit_code
                                    ),
                                    &[
                                        PhpMixed::String(callable_str.clone()),
                                        PhpMixed::String(event.get_name().to_string()),
                                    ],
                                )),
                                true,
                                <dyn IOInterface>::QUIET,
                            );

                            return Err(anyhow::anyhow!(ScriptExecutionException(
                                RuntimeException {
                                    message: format!(
                                        "Error Output: {}",
                                        self.process.get_error_output()
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
                            self.io.write_error(
                                PhpMixed::String(sprintf(
                                    "<warning>You made a reference to a non-existent script %s</warning>",
                                    &[PhpMixed::String(callable_str.clone())],
                                )),
                                true,
                                <dyn IOInterface>::QUIET,
                            );
                        }

                        let composer_full = self.composer_as_full_or_panic();
                        let mut script_event = ScriptEvent::new(
                            script_name.clone(),
                            composer_full,
                            self.io_clone(),
                            // event.isDevMode() is only on InstallerEvent/ScriptEvent/PackageEvent
                            // TODO(plugin): proper dev_mode propagation when polymorphic event is supported
                            false,
                            args,
                            flags,
                        );
                        // TODO(plugin): script_event.set_originating_event(event.clone())
                        match self.dispatch(Some(&script_name), Some(Event::new(
                            script_name.clone(),
                            script_event.inner_args_for_dispatch(),
                            script_event.inner_flags_for_dispatch(),
                        ))) {
                            Ok(v) => r#return = v,
                            Err(e) => {
                                if e.downcast_ref::<ScriptExecutionException>().is_some() {
                                    self.io.write_error(
                                        PhpMixed::String(sprintf(
                                            "<error>Script %s was called via %s</error>",
                                            &[
                                                PhpMixed::String(callable_str.clone()),
                                                PhpMixed::String(event.get_name().to_string()),
                                            ],
                                        )),
                                        true,
                                        <dyn IOInterface>::QUIET,
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

                    self.make_autoloader(event, &Callable::String(callable_str.clone()));
                    if !class_exists(&class_name) {
                        self.io.write_error(
                            PhpMixed::String(format!(
                                "<warning>Class {} is not autoloadable, can not call {} script</warning>",
                                class_name,
                                event.get_name()
                            )),
                            true,
                            <dyn IOInterface>::QUIET,
                        );
                        continue;
                    }
                    if !is_callable(&PhpMixed::String(callable_str.clone())) {
                        self.io.write_error(
                            PhpMixed::String(format!(
                                "<warning>Method {} is not callable, can not call {} script</warning>",
                                callable_str,
                                event.get_name()
                            )),
                            true,
                            <dyn IOInterface>::QUIET,
                        );
                        continue;
                    }

                    match self.execute_event_php_script(&class_name, &method_name, event) {
                        Ok(v) => {
                            r#return = if let PhpMixed::Bool(false) = v { 1 } else { 0 };
                        }
                        Err(e) => {
                            let message =
                                "Script %s handling the %s event terminated with an exception";
                            self.io.write_error(
                                PhpMixed::String(format!(
                                    "<error>{}</error>",
                                    sprintf(
                                        message,
                                        &[
                                            PhpMixed::String(callable_str.clone()),
                                            PhpMixed::String(event.get_name().to_string()),
                                        ],
                                    )
                                )),
                                true,
                                <dyn IOInterface>::QUIET,
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
                    );
                    if !class_exists(&class_name) {
                        self.io.write_error(
                            PhpMixed::String(format!(
                                "<warning>Class {} is not autoloadable, can not call {} script</warning>",
                                class_name,
                                event.get_name()
                            )),
                            true,
                            <dyn IOInterface>::QUIET,
                        );
                        continue;
                    }
                    if !is_a(
                        &PhpMixed::String(class_name.clone()),
                        "Symfony\\Component\\Console\\Command\\Command",
                        true,
                    ) {
                        self.io.write_error(
                            PhpMixed::String(format!(
                                "<warning>Class {} does not extend Symfony\\Component\\Console\\Command\\Command, can not call {} script</warning>",
                                class_name,
                                event.get_name()
                            )),
                            true,
                            <dyn IOInterface>::QUIET,
                        );
                        continue;
                    }
                    if defined(&format!(
                        "Composer\\Script\\ScriptEvents::{}",
                        str_replace("-", "_", &strtoupper(event.get_name()))
                    )) {
                        self.io.write_error(
                            PhpMixed::String(format!(
                                "<warning>You cannot bind {} to a Command class, use a non-reserved name</warning>",
                                event.get_name()
                            )),
                            true,
                            <dyn IOInterface>::QUIET,
                        );
                        continue;
                    }

                    let mut app = Application::new();
                    app.set_catch_exceptions(false);
                    if method_exists(
                        &PhpMixed::String("Application".to_string()),
                        "setCatchErrors",
                    ) {
                        app.set_catch_errors(false);
                    }
                    app.set_auto_exit(false);
                    // TODO(plugin): instantiate command class dynamically: `new $className($event->getName())`
                    let cmd = Command::new(event.get_name().to_string());
                    if method_exists(&PhpMixed::String("Application".to_string()), "addCommand") {
                        app.add_command(cmd.clone());
                    } else {
                        // Compatibility layer for symfony/console <7.4
                        app.add(cmd.clone());
                    }
                    app.set_default_command(cmd.get_name().to_string(), true);
                    let result = (|| -> anyhow::Result<i64> {
                        let args = additional_args
                            .iter()
                            .map(|arg| ProcessExecutor::escape(arg))
                            .collect::<Vec<_>>()
                            .join(" ");
                        // reusing the output from $this->io is mostly needed for tests, but generally speaking
                        // it does not hurt to keep the same stream as the current Application
                        let output = if let Some(_console_io) =
                            self.io.as_any().downcast_ref::<ConsoleIO>()
                        {
                            // TODO(plugin): \ReflectionProperty to read private `output` from ConsoleIO
                            // is required by the original PHP — needs user-decided porting strategy.
                            let _refl_php_version_gate = PHP_VERSION_ID < 80100;
                            todo!("\\ReflectionProperty on ConsoleIO::$output")
                        } else {
                            ConsoleOutput::new()
                        };
                        let input_str = event
                            .get_flags()
                            .get("script-alias-input")
                            .and_then(|v| v.as_string())
                            .unwrap_or(&args)
                            .to_string();
                        Ok(app.run(StringInput::new(input_str), output))
                    })();
                    match result {
                        Ok(v) => r#return = v,
                        Err(e) => {
                            let message =
                                "Script %s handling the %s event terminated with an exception";
                            self.io.write_error(
                                PhpMixed::String(format!(
                                    "<error>{}</error>",
                                    sprintf(
                                        message,
                                        &[
                                            PhpMixed::String(callable_str.clone()),
                                            PhpMixed::String(event.get_name().to_string()),
                                        ],
                                    )
                                )),
                                true,
                                <dyn IOInterface>::QUIET,
                            );
                            return Err(e);
                        }
                    }
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
                            if args == "" {
                                "".to_string()
                            } else {
                                format!(" {}", args)
                            }
                        )
                    };

                    if self.io.is_verbose() {
                        self.io.write_error(
                            PhpMixed::String(sprintf(
                                "> %s: %s",
                                &[
                                    PhpMixed::String(event.get_name().to_string()),
                                    PhpMixed::String(exec.clone()),
                                ],
                            )),
                            true,
                            <dyn IOInterface>::NORMAL,
                        );
                    } else if self.event_needs_to_output(event) {
                        self.io.write_error(
                            PhpMixed::String(sprintf(
                                "> %s",
                                &[PhpMixed::String(exec.clone())],
                            )),
                            true,
                            <dyn IOInterface>::NORMAL,
                        );
                    }

                    let possible_local_binaries =
                        self.composer.get_package().get_binaries();
                    if !possible_local_binaries.is_empty() {
                        for local_exec in &possible_local_binaries {
                            if Preg::is_match(
                                &format!("{{\\b{}$}}", preg_quote(&callable_str, None)),
                                local_exec,
                            )
                            .unwrap_or(false)
                            {
                                let caller = BinaryInstaller::determine_binary_caller(local_exec);
                                exec = Preg::replace(
                                    &format!("{{^{}}}", preg_quote(&callable_str, None)),
                                    &format!("{} {}", caller, local_exec),
                                    &exec,
                                )
                                .unwrap_or(exec);
                                break;
                            }
                        }
                    }

                    if strpos(&exec, "@putenv ") == Some(0) {
                        if strpos(&exec, "=").is_none() {
                            Platform::clear_env(&substr(&exec, 8, None));
                        } else {
                            let parts: Vec<&str> =
                                substr(&exec, 8, None).splitn(2, '=').collect::<Vec<_>>()
                                    .iter()
                                    .map(|s| *s)
                                    .collect();
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
                                "{^\\S+}",
                                |m| str_replace("/", "\\", &m[0]),
                                &path_and_args,
                            )
                            .unwrap_or(path_and_args);
                        }
                        // match somename (not in quote, and not a qualified path) and if it is not a valid path from CWD then try to find it
                        // in $PATH. This allows support for `@php foo` where foo is a binary name found in PATH but not an actual relative path
                        let mat = Preg::is_match_strict_groups(
                            "{^[^\\'\"\\s/\\\\]+}",
                            &path_and_args,
                        )
                        .ok()
                        .flatten();
                        if let Some(m) = mat {
                            if !file_exists(&m[0]) {
                                let finder = ExecutableFinder::new();
                                if let Some(path_to_exec) = finder.find(&m[0]) {
                                    let mut path_to_exec = path_to_exec;
                                    if Platform::is_windows() {
                                        let exec_without_ext = Preg::replace(
                                            "{\\.(exe|bat|cmd|com)$}i",
                                            "",
                                            &path_to_exec,
                                        )
                                        .unwrap_or(path_to_exec.clone());
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
                                "{^\\S+}",
                                |m| str_replace("/", "\\", &m[0]),
                                &exec,
                            )
                            .unwrap_or(exec);
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
                        self.io.write_error(
                            PhpMixed::String(sprintf(
                                &format!(
                                    "<error>Script %s handling the %s event returned with error code {}</error>",
                                    exit_code
                                ),
                                &[
                                    PhpMixed::String(callable_str.clone()),
                                    PhpMixed::String(event.get_name().to_string()),
                                ],
                            )),
                            true,
                            <dyn IOInterface>::QUIET,
                        );

                        return Err(anyhow::anyhow!(ScriptExecutionException(
                            RuntimeException {
                                message: format!(
                                    "Error Output: {}",
                                    self.process.get_error_output()
                                ),
                                code: exit_code,
                            }
                        )));
                    }
                }
                _ => {
                    // unreachable in practice — the first match arm guard handles non-string callables.
                }
            } }

            return_max = max_i64(return_max, r#return);

            if event.is_propagation_stopped() {
                break;
            }
        }
        Ok(return_max)
    }

    fn do_dispatch_script(&mut self, event: ScriptEvent) -> anyhow::Result<i64> {
        // TODO(plugin): proper polymorphic dispatch — currently delegate to base Event path.
        let base = Event::new(
            event.get_inner().get_name().to_string(),
            event.get_inner().get_arguments().clone(),
            event.get_inner().get_flags().clone(),
        );
        self.do_dispatch(base)
    }

    fn do_dispatch_package(&mut self, event: PackageEvent) -> anyhow::Result<i64> {
        // TODO(plugin): preserve PackageEvent identity for `instanceof` checks above.
        let base = Event::new(
            event.get_name().to_string(),
            Vec::new(),
            IndexMap::new(),
        );
        self.do_dispatch(base)
    }

    fn do_dispatch_installer(&mut self, event: InstallerEvent) -> anyhow::Result<i64> {
        // TODO(plugin): preserve InstallerEvent identity for `instanceof` checks above.
        let base = Event::new(
            event.get_inner_name().to_string(),
            Vec::new(),
            IndexMap::new(),
        );
        self.do_dispatch(base)
    }

    fn execute_tty(&self, exec: &str) -> anyhow::Result<i64> {
        if self.io.is_interactive() {
            return self.process.execute_tty(exec);
        }

        self.process.execute(exec)
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
        let php_args = if php_args.len() > 0 {
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
        event: &Event,
    ) -> anyhow::Result<PhpMixed> {
        if self.io.is_verbose() {
            self.io.write_error(
                PhpMixed::String(sprintf(
                    "> %s: %s::%s",
                    &[
                        PhpMixed::String(event.get_name().to_string()),
                        PhpMixed::String(class_name.to_string()),
                        PhpMixed::String(method_name.to_string()),
                    ],
                )),
                true,
                <dyn IOInterface>::NORMAL,
            );
        } else if self.event_needs_to_output(event) {
            self.io.write_error(
                PhpMixed::String(sprintf(
                    "> %s::%s",
                    &[
                        PhpMixed::String(class_name.to_string()),
                        PhpMixed::String(method_name.to_string()),
                    ],
                )),
                true,
                <dyn IOInterface>::NORMAL,
            );
        }

        // TODO(plugin): invoke `$className::$methodName($event)` dynamically
        todo!("dynamic static method invocation requires plugin runtime")
    }

    fn event_needs_to_output(&self, event: &Event) -> bool {
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
            .or_insert_with(IndexMap::new)
            .entry(priority)
            .or_insert_with(Vec::new)
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
    pub fn add_subscriber<S: EventSubscriberInterface>(&mut self, _subscriber: &S) {
        // TODO(plugin): port full subscriber registration — depends on dynamic dispatch
        // for `[$subscriber, $methodName]` style callables.
        for (event_name, _params) in S::get_subscribed_events() {
            let _ = event_name;
        }
    }

    /// Retrieves all listeners for a given event
    fn get_listeners(&mut self, event: &Event) -> Vec<Callable> {
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
                .or_insert_with(IndexMap::new)
                .insert(0, Vec::new());
        }
        if let Some(priorities) = self.listeners.get_mut(&name) {
            krsort(priorities);
        }

        let mut listeners = self.listeners.clone();
        if let Some(priorities) = listeners.get_mut(&name) {
            if let Some(zero_list) = priorities.get_mut(&0) {
                zero_list.extend(script_listeners);
            }
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
    pub fn has_event_listeners(&mut self, event: &Event) -> bool {
        let listeners = self.get_listeners(event);

        listeners.len() > 0
    }

    /// Finds all listeners defined as scripts in the package
    fn get_script_listeners(&self, event: &Event) -> Vec<Callable> {
        let package = self.composer.get_package();
        let scripts = package.get_scripts();

        let event_scripts = match scripts.get(event.get_name()) {
            Some(v) if !Self::is_empty_value(v) => v.clone(),
            _ => return Vec::new(),
        };

        if self
            .skip_scripts
            .iter()
            .any(|s| s == event.get_name())
        {
            self.io.write_error(
                PhpMixed::String(format!(
                    "Skipped script listeners for <info>{}</info> because of COMPOSER_SKIP_SCRIPTS",
                    event.get_name()
                )),
                true,
                <dyn IOInterface>::VERBOSE,
            );

            return Vec::new();
        }

        // PHP returns the array of script strings; convert each to Callable::String
        match event_scripts {
            PhpMixed::Array(map) => map
                .values()
                .filter_map(|v| match v.as_ref() {
                    PhpMixed::String(s) => Some(Callable::String(s.clone())),
                    _ => None,
                })
                .collect(),
            PhpMixed::List(list) => list
                .iter()
                .filter_map(|v| match v.as_ref() {
                    PhpMixed::String(s) => Some(Callable::String(s.clone())),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
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
    fn push_event(&mut self, event: &Event) -> anyhow::Result<i64> {
        let event_name = event.get_name().to_string();
        if self.event_stack.iter().any(|n| n == &event_name) {
            return Err(anyhow::anyhow!(RuntimeException {
                message: sprintf(
                    "Circular call to script handler '%s' detected",
                    &[PhpMixed::String(event_name)],
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
            .composer
            .get_config()
            .get("bin-dir")
            .and_then(|v| match v {
                PhpMixed::String(s) => Some(s),
                _ => None,
            })
            .unwrap_or_default();
        if shirabe_php_shim::is_dir(&bin_dir) {
            let bin_dir = realpath(&bin_dir).unwrap_or(bin_dir);
            let path_value = Platform::get_env(path_env).unwrap_or_default();
            if !Preg::is_match(
                &format!(
                    "{{(^|{}){}($|{})}}",
                    PATH_SEPARATOR,
                    preg_quote(&bin_dir, None),
                    PATH_SEPARATOR
                ),
                &path_value,
            )
            .unwrap_or(false)
            {
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
        if is_array(cb) {
            if let PhpMixed::Array(map) = cb {
                let entries: Vec<&Box<PhpMixed>> = map.values().collect();
                if entries.len() >= 2 {
                    let first = entries[0].as_ref();
                    let second = entries[1].as_ref();
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
        }

        // not great but also do not want to break everything here
        "unsupported".to_string()
    }

    fn make_autoloader(&mut self, event: &Event, callable: &Callable) {
        // TODO(plugin): full autoloader rebuild on plugin-supplied callables — currently a stub.
        let composer = match self.composer_as_full() {
            Some(c) => c,
            None => return,
        };

        let callable_key = match callable {
            Callable::ArrayCallable(first, method) => {
                let prefix = if let PhpMixed::String(s) = first.as_ref() {
                    s.clone()
                } else {
                    get_class(first.as_ref())
                };
                format!("{}::{}", prefix, method)
            }
            Callable::String(s) => s.clone(),
            Callable::Closure => "closure".to_string(),
        };
        if self.previous_listeners.contains_key(&callable_key) {
            return;
        }
        self.previous_listeners.insert(callable_key, true);

        let package = composer.get_package();
        let packages = composer
            .get_repository_manager()
            .get_local_repository()
            .get_canonical_packages();
        let mut generator = composer.get_autoload_generator();
        let mut hash_input = packages
            .iter()
            .map(|p: &Box<dyn PackageInterface>| format!("{}/{}", p.get_name(), p.get_version()))
            .collect::<Vec<_>>()
            .join(",");
        // TODO(plugin): polymorphic isDevMode propagation for ScriptEvent / PackageEvent / InstallerEvent
        let _ = event;
        hash_input.push_str("");
        let hash_value = hash("sha256", &hash_input);

        if self.previous_hash.as_deref() == Some(hash_value.as_str()) {
            return;
        }

        self.previous_hash = Some(hash_value);

        let package_map = generator.build_package_map(
            composer.get_installation_manager(),
            package,
            &packages,
        );
        let map = generator.parse_autoloads(&package_map, package);

        if self.loader.is_some() {
            self.loader.as_mut().unwrap().unregister();
        }

        let vendor_dir = composer
            .get_config()
            .get("vendor-dir")
            .and_then(|v| match v {
                PhpMixed::String(s) => Some(s),
                _ => None,
            })
            .unwrap_or_default();
        let mut loader = generator.create_loader(&map, &vendor_dir);
        loader.register(false);
        self.loader = Some(loader);
    }

    // ---- helpers ----

    fn io_clone(&self) -> Box<dyn IOInterface> {
        // TODO(phase-b): IOInterface is not Clone — placeholder until io ownership is resolved.
        todo!("clone Box<dyn IOInterface>")
    }

    fn composer_as_full(&self) -> Option<&Composer> {
        // TODO(phase-b): PartialComposer ↔ Composer downcasting requires Phase B design.
        None
    }

    fn composer_as_full_or_panic(&self) -> Composer {
        // assert($this->composer instanceof Composer, ...)
        assert!(
            self.composer_as_full().is_some(),
            "This should only be reached with a fully loaded Composer"
        );
        let _ = LogicException {
            message: "This should only be reached with a fully loaded Composer".to_string(),
            code: 0,
        };
        todo!("clone Composer out of PartialComposer in Phase B")
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

// TODO(plugin): re-export the `Event::name`-only constructor `Event::new` PHP variant so callers
// can build an `Event` from just a name, mirroring `new Event($eventName)`.
impl Event {
    pub fn from_name(name: String) -> Self {
        Event::new(name, Vec::new(), IndexMap::new())
    }
}

// Convenience accessors that ScriptEvent doesn't currently expose for the base Event fields.
// TODO(plugin): replace with proper getters once ScriptEvent grows them.
impl ScriptEvent {
    fn get_inner(&self) -> &Event {
        unimplemented!("ScriptEvent::get_inner — Phase B")
    }

    fn inner_args_for_dispatch(&self) -> Vec<String> {
        Vec::new()
    }

    fn inner_flags_for_dispatch(&self) -> IndexMap<String, PhpMixed> {
        IndexMap::new()
    }
}

// Convenience accessor for InstallerEvent's underlying name.
impl InstallerEvent {
    fn get_inner_name(&self) -> &str {
        unimplemented!("InstallerEvent::get_inner_name — Phase B")
    }
}

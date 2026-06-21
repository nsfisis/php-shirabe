//! ref: composer/src/Composer/EventDispatcher/EventDispatcher.php

use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::symfony::process::ExecutableFinder;
use shirabe_external_packages::symfony::process::PhpExecutableFinder;
use shirabe_php_shim::{
    Exception, InvalidArgumentException, LogicException, PATH_SEPARATOR, PHP_VERSION_ID, PhpMixed,
    RuntimeException, array_pop, array_push, array_search_in_vec, array_splice, class_exists,
    count_mixed, defined, file_exists, get_class, hash, implode, ini_get, is_a, is_array,
    is_callable, is_object, is_string, krsort, method_exists, preg_quote, realpath,
    spl_autoload_functions, spl_autoload_register, spl_autoload_unregister, spl_object_hash,
    sprintf, str_contains, str_ends_with, str_replace, str_starts_with, strlen, strpos, strtoupper,
    substr, trim,
};

use crate::autoload::ClassLoader;
use crate::composer::PartialComposerHandle;
use crate::composer::PartialComposerWeakHandle;
use crate::dependency_resolver::Transaction;
use crate::dependency_resolver::operation::OperationInterface;
use crate::event_dispatcher::Event;
use crate::event_dispatcher::EventInterface;
use crate::event_dispatcher::EventSubscriberInterface;
use crate::event_dispatcher::ScriptExecutionException;
use crate::installer::BinaryInstaller;
use crate::installer::InstallerEvent;
use crate::installer::PackageEvent;
use crate::io::ConsoleIO;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::plugin::CommandEvent;
use crate::plugin::PreCommandRunEvent;
use crate::repository::RepositoryInterface;
use crate::script::Event as ScriptEvent;
use crate::util::Platform;
use crate::util::ProcessExecutor;

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
        operations: Vec<std::rc::Rc<dyn OperationInterface>>,
        operation: std::rc::Rc<dyn OperationInterface>,
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
                let replaced = Preg::replace("{ ?@no_additional_args}", "", s);
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
            if !is_string_callable {
                // TODO(plugin): non-string callable handling — verify is_callable, invoke,
                // and replicate the get_class / write_error / is_callable error path from PHP.
                let _ = self.make_autoloader(event, &callable);
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
                            "> {}: {}",
                            PhpMixed::String(formatted_event_name_with_args.clone()),
                            PhpMixed::String(format!("{}->{}", prefix, method_name)),
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
                                PhpMixed::String(formatted_event_name_with_args.clone()),
                                PhpMixed::String(callable_str.clone()),
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
                                    PhpMixed::String(callable_str.clone()),
                                    PhpMixed::String(event.get_name().to_string()),
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
                                    PhpMixed::String(callable_str.clone()),
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
                                                PhpMixed::String(callable_str.clone()),
                                                PhpMixed::String(event.get_name().to_string()),
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

                        let _ =
                            self.make_autoloader(event, &Callable::String(callable_str.clone()));
                        if !class_exists(&class_name) {
                            self.io.write_error3(&format!(
                                "<warning>Class {} is not autoloadable, can not call {} script</warning>",
                                class_name,
                                event.get_name()
                            ), true, crate::io::QUIET);
                            continue;
                        }
                        if !is_callable(&PhpMixed::String(callable_str.clone())) {
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
                                        PhpMixed::String(callable_str.clone()),
                                        PhpMixed::String(event.get_name().to_string()),
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

                        let _ = self.make_autoloader(
                            event,
                            &Callable::ArrayCallable(
                                Box::new(PhpMixed::String(callable_str.clone())),
                                "run".to_string(),
                            ),
                        );
                        if !class_exists(&class_name) {
                            self.io.write_error3(&format!(
                                "<warning>Class {} is not autoloadable, can not call {} script</warning>",
                                class_name,
                                event.get_name()
                            ), true, crate::io::QUIET);
                            continue;
                        }
                        if !is_a(
                            &PhpMixed::String(class_name.clone()),
                            "Symfony\\Component\\Console\\Command\\Command",
                            true,
                        ) {
                            self.io.write_error3(&format!(
                                "<warning>Class {} does not extend Symfony\\Component\\Console\\Command\\Command, can not call {} script</warning>",
                                class_name,
                                event.get_name()
                            ), true, crate::io::QUIET);
                            continue;
                        }
                        if defined(&format!(
                            "Composer\\Script\\ScriptEvents::{}",
                            str_replace("-", "_", &strtoupper(event.get_name()))
                        )) {
                            self.io.write_error3(&format!(
                                "<warning>You cannot bind {} to a Command class, use a non-reserved name</warning>",
                                event.get_name()
                            ), true, crate::io::QUIET);
                            continue;
                        }

                        // PHP hosts the user's Command class in a throwaway, bare
                        // `Symfony\Component\Console\Application` (NOT Composer's Application):
                        //   $app = new Application();
                        //   $app->setCatchExceptions(false);
                        //   $app->setAutoExit(false);
                        //   $cmd = new $className($event->getName());
                        //   $app->add($cmd);
                        //   $app->setDefaultCommand((string) $cmd->getName(), true);
                        //   $return = $app->run(new StringInput(...), $output);
                        //
                        // TODO(plugin): a `scripts` entry naming a Symfony Command subclass is run by
                        // hosting it in a bare Symfony console Application. This requires the PHP
                        // runtime — both the dynamic `new $className(...)` instantiation and the real
                        // Symfony Application. It will be implemented by generating a PHP bootstrap
                        // (the boilerplate above) parameterized by the class name, event name and
                        // args, then executing it via the PHP runtime with the child process
                        // inheriting STDOUT/STDERR in place of reusing the in-memory output. No
                        // Rust-side Symfony Application is involved, so none is constructed here.
                        let _ = &additional_args;
                        todo!(
                            "plugin: run a `scripts` Command class via the PHP runtime (bare Symfony Application host)"
                        );
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
                                &format!(
                                    "> {}: {}",
                                    PhpMixed::String(event.get_name().to_string()),
                                    PhpMixed::String(exec.clone()),
                                ),
                                true,
                                crate::io::NORMAL,
                            );
                        } else if self.event_needs_to_output(event) {
                            self.io.write_error3(
                                &format!("> {}", PhpMixed::String(exec.clone())),
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
                                    &format!("{{\\b{}$}}", preg_quote(&callable_str, None)),
                                    local_exec,
                                ) {
                                    let caller =
                                        BinaryInstaller::determine_binary_caller(local_exec);
                                    exec = Preg::replace(
                                        &format!("{{^{}}}", preg_quote(&callable_str, None)),
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
                                    "{^\\S+}",
                                    |m| str_replace("/", "\\", &m[0]),
                                    &path_and_args,
                                );
                            }
                            // match somename (not in quote, and not a qualified path) and if it is not a valid path from CWD then try to find it
                            // in $PATH. This allows support for `@php foo` where foo is a binary name found in PATH but not an actual relative path
                            let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                            if Preg::is_match3("{^[^\\'\"\\s/\\\\]+}", &path_and_args, Some(&mut m))
                            {
                                let m0 =
                                    m.get(&CaptureKey::ByIndex(0)).cloned().unwrap_or_default();
                                if !file_exists(&m0) {
                                    let finder = ExecutableFinder::new();
                                    if let Some(path_to_exec) = finder.find(&m0, None, &[]) {
                                        let mut path_to_exec = path_to_exec;
                                        if Platform::is_windows() {
                                            let exec_without_ext = Preg::replace(
                                                "{\\.(exe|bat|cmd|com)$}i",
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
                                    "{^\\S+}",
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
                                PhpMixed::String(callable_str.clone()),
                                PhpMixed::String(event.get_name().to_string()),
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
            return self.process.borrow_mut().execute_tty(exec, ());
        }

        self.process.borrow_mut().execute(exec, (), ())
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
                &format!(
                    "> {}: {}::{}",
                    PhpMixed::String(event.get_name().to_string()),
                    PhpMixed::String(class_name.to_string()),
                    PhpMixed::String(method_name.to_string()),
                ),
                true,
                crate::io::NORMAL,
            );
        } else if self.event_needs_to_output(event) {
            self.io.write_error3(
                &format!(
                    "> {}::{}",
                    PhpMixed::String(class_name.to_string()),
                    PhpMixed::String(method_name.to_string()),
                ),
                true,
                crate::io::NORMAL,
            );
        }

        // TODO(plugin): invoke `$className::$methodName($event)` dynamically
        todo!("dynamic static method invocation requires plugin runtime")
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
    pub fn add_subscriber<S: EventSubscriberInterface>(&mut self, _subscriber: &S) {
        // TODO(plugin): port full subscriber registration — depends on dynamic dispatch
        // for `[$subscriber, $methodName]` style callables.
        for (event_name, _params) in S::get_subscribed_events() {
            let _ = event_name;
        }
    }

    /// Retrieves all listeners for a given event
    fn get_listeners(&mut self, event: &dyn EventInterface) -> Vec<Callable> {
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
            .borrow_mut()
            .get("bin-dir")
            .as_string()
            .map(|s| s.to_string())
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
        // TODO(plugin): full autoloader rebuild on plugin-supplied callables — currently a stub.
        let Some(composer) = composer.as_full() else {
            return Ok(());
        };
        let composer = composer.borrow_mut();

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
            return Ok(());
        }
        self.previous_listeners.insert(callable_key, true);

        let package = composer.get_package();
        let packages = composer
            .get_repository_manager()
            .borrow()
            .get_local_repository()
            .get_canonical_packages()?;
        let generator = composer.get_autoload_generator().clone();
        let generator = generator.borrow();
        let mut hash_input = packages
            .iter()
            .map(|p: &crate::package::PackageInterfaceHandle| {
                format!("{}/{}", p.get_name(), p.get_version())
            })
            .collect::<Vec<_>>()
            .join(",");
        // TODO(plugin): polymorphic isDevMode propagation for ScriptEvent / PackageEvent / InstallerEvent
        let _ = event;
        hash_input.push_str("");
        let hash_value = hash("sha256", &hash_input);

        if self.previous_hash.as_deref() == Some(hash_value.as_str()) {
            return Ok(());
        }

        self.previous_hash = Some(hash_value);

        let installation_manager = composer.get_installation_manager();
        let package_map = generator.build_package_map(
            &mut installation_manager.borrow_mut(),
            package.clone(),
            packages,
        )?;
        let map = generator.parse_autoloads(
            package_map,
            package.clone(),
            shirabe_php_shim::PhpMixed::Bool(false),
        );

        if self.loader.is_some() {
            self.loader.as_mut().unwrap().unregister();
        }

        let vendor_dir = composer
            .get_config()
            .borrow_mut()
            .get("vendor-dir")
            .as_string()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let mut loader = generator.create_loader(&map, Some(vendor_dir.clone()));
        loader.register(false);
        self.loader = Some(loader);
        Ok(())
    }

    // ---- helpers ----

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

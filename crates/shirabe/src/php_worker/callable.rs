//! The PHP callables `EventDispatcher` invokes in the worker: a subscriber's listener method, a
//! `Class::method` script, and a `scripts` Command class.

use crate::event_dispatcher::EventInterface;
use crate::php_worker::call::PhpCall;
use crate::php_worker::rpc::dispatch::{PluginRpcDispatcher, ScriptRpcDispatcher};
use crate::php_worker::rpc::table::{
    RustEntity, event_stub_class, register_entity, rust_handle_value,
};
use crate::php_worker::runtime::{ensure_composer_php_runtime, ensure_script_autoloader};
use crate::php_worker::throwable::throw_into_error;
use bstr::{BStr, BString};
use shirabe_php_rpc::{PhpObjHandle, PhpThrow, PluginValue};
use shirabe_php_shim::{PhpMixed, RuntimeException, bformat, var_export_str};

/// The event as the argument a PHP callable receives: a proxy stub of the event's class.
fn event_argument(
    event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
    callee: BString,
) -> anyhow::Result<PluginValue> {
    let stub_class = event_stub_class(&*event.borrow()).ok_or_else(|| -> anyhow::Error {
        // TODO(plugin): an event class without a proxy stub cannot cross the boundary.
        RuntimeException::new(bformat!(
            "no proxy stub is available yet for the event `{}` dispatched to {callee}",
            event.borrow().get_name()
        ))
        .into()
    })?;
    Ok(rust_handle_value(
        register_entity(RustEntity::Event(event.clone())),
        stub_class,
    ))
}

/// `is_callable([$object, $method])` for a subscriber's listener, with the script autoloader
/// active.
pub(crate) fn is_callable_method(
    handle: &PhpObjHandle,
    method_name: &BStr,
) -> anyhow::Result<bool> {
    ensure_script_autoloader()?;
    PhpCall::Function(BStr::new("is_callable")).run(vec![PluginValue::List(vec![
        PluginValue::PhpHandle(handle.clone()),
        PluginValue::string(method_name),
    ])])
}

/// `$object->$method($event)` for a subscriber's listener. Answers whether the listener returned
/// `false`.
pub(crate) fn call_method_with_event(
    handle: &PhpObjHandle,
    method_name: &BStr,
    event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
) -> anyhow::Result<bool> {
    let argument = event_argument(event, bformat!("{}::{method_name}", handle.class()))?;
    let value: PluginValue =
        PhpCall::Method(handle, method_name).run_with(vec![argument], &mut PluginRpcDispatcher)?;
    Ok(matches!(value, PluginValue::Bool(false)))
}

/// `$className::$methodName($event)` for a `Class::method` script.
pub(crate) fn call_static_with_event(
    class_name: &BStr,
    method_name: &BStr,
    event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
    cache_dir: impl FnOnce() -> anyhow::Result<std::path::PathBuf>,
) -> anyhow::Result<PhpMixed> {
    let argument = event_argument(event, bformat!("{class_name}::{method_name}"))?;

    // The event stub the script receives hands out stubs of the Composer object graph, and
    // those extend and implement the real Composer contracts (`PackageInterface` and the
    // rest), which only the Composer PHP runtime carries.
    ensure_composer_php_runtime(cache_dir)?;
    ensure_script_autoloader()?;
    PhpCall::StaticMethod(class_name, method_name)
        .run_with(vec![argument], &mut ScriptRpcDispatcher)
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

/// How a `scripts` Command class run by [`run_command_script`] ended.
#[derive(Debug)]
pub(crate) struct CommandScriptOutcome {
    /// The exit status, `None` when the run threw.
    pub(crate) status: Option<i64>,
    /// What the command wrote to its output.
    pub(crate) output: BString,
    /// The exception the run ended with.
    pub(crate) throw: Option<anyhow::Error>,
    /// Whether Composer would have caught [`Self::throw`] (it is an `\Exception`).
    pub(crate) caught: bool,
}

/// Runs the `scripts` Command class `class_name` for the event `event_name`. PHP hosts the
/// command in a throwaway, bare `Symfony\Component\Console\Application` (NOT Composer's
/// Application), built by a generated snippet running inside the worker.
///
/// TODO(plugin): the BufferedOutput has to go. The command has to run against the real output
/// stream, the way upstream hands the live output object of `$this->io` to `$app->run()`;
/// collecting the output and writing the buffer back once the run has returned is not a
/// substitute.
pub(crate) fn run_command_script(
    class_name: &BStr,
    event_name: &BStr,
    verbosity: i64,
    decorated: bool,
    input: &BStr,
) -> anyhow::Result<CommandScriptOutcome> {
    let snippet = bformat!(
        "\n$className = {};\n$app = new \\Symfony\\Component\\Console\\Application();\n$app->setCatchExceptions(false);\nif (method_exists($app, 'setCatchErrors')) {{\n    $app->setCatchErrors(false);\n}}\n$app->setAutoExit(false);\n$cmd = new $className({});\nif (method_exists($app, 'addCommand')) {{\n    $app->addCommand($cmd);\n}} else {{\n    $app->add($cmd);\n}}\n$app->setDefaultCommand((string) $cmd->getName(), true);\n$output = new \\Symfony\\Component\\Console\\Output\\BufferedOutput({verbosity}, {});\ntry {{\n    $status = $app->run(new \\Symfony\\Component\\Console\\Input\\StringInput({}), $output);\n    return ['status' => $status, 'output' => $output->fetch(), 'throw' => null, 'caught' => false];\n}} catch (\\Throwable $e) {{\n    return ['status' => null, 'output' => $output->fetch(), 'throw' => \\Shirabe\\MaterializedThrowable::describe($e), 'caught' => $e instanceof \\Exception];\n}}\n",
        var_export_str(class_name, true),
        var_export_str(event_name, true),
        if decorated { "true" } else { "false" },
        var_export_str(input, true)
    );
    ensure_script_autoloader()?;
    let result: ScriptCommandResult = PhpCall::Function(BStr::new("__shirabe_eval"))
        .run_with(vec![PluginValue::string(snippet)], &mut ScriptRpcDispatcher)?;
    Ok(CommandScriptOutcome {
        status: result.status,
        output: result.output,
        throw: result
            .throw
            .map(|throw| throw_into_error(PhpThrow::from_wire(throw))),
        caught: result.caught,
    })
}

//! ref: composer/tests/Composer/Test/ApplicationTest.php

// These drive the console Application (doRun, command resolution, plugin disabling).
// The tests exercising do_run's script-command registration (a todo!() pending the
// Symfony command-registry model), or a runtime define() of COMPOSER_DEV_WARNING_TIME,
// remain unportable.

#[path = "common/bootstrap.rs"]
mod bootstrap;
#[path = "common/test_case.rs"]
mod test_case;

use serial_test::serial;
use test_case::init_temp_composer;

use shirabe::command::about_command::AboutCommand;
use shirabe::command::self_update_command::SelfUpdateCommand;
use shirabe::console::application::ApplicationHandle;
use shirabe::util::platform::Platform;
use shirabe_external_packages::symfony::console::command::Command;
use shirabe_external_packages::symfony::console::input::array_input::ArrayInput;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::buffered_output::BufferedOutput;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

fn set_up() {
    Platform::put_env("COMPOSER_DISABLE_XDEBUG_WARN", "1");
}

fn tear_down() {
    Platform::clear_env("COMPOSER_DISABLE_XDEBUG_WARN");
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

#[ignore = "no define() setter exists for the COMPOSER_DEV_WARNING_TIME constant (shim defined() is a fixed matches!)"]
#[test]
fn test_dev_warning() {
    let _tear_down = TearDown;
    set_up();

    // TODO(phase-d): no define() setter exists for the COMPOSER_DEV_WARNING_TIME constant (the
    // shim's defined() is a fixed matches!), so this test's runtime define() cannot be reproduced.
    todo!()
}

#[ignore = "SelfUpdateCommand::execute is intentionally stubbed with a Shirabe-specific \"not available\" message instead of the original Composer wording this test expects"]
#[test]
fn test_dev_warning_suppressed_for_self_update() {
    let _tear_down = TearDown;
    set_up();

    if Platform::is_windows() {
        // markTestSkipped('Does not run on windows')
        return;
    }

    let application = ApplicationHandle::new("Composer".to_string(), "".to_string()).unwrap();
    let command: std::rc::Rc<std::cell::RefCell<dyn Command>> =
        std::rc::Rc::new(std::cell::RefCell::new(SelfUpdateCommand::new()));
    application.add(command).unwrap();

    let output = std::rc::Rc::new(std::cell::RefCell::new(BufferedOutput::new(
        None, false, None,
    )));
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            ArrayInput::new(
                vec![(PhpMixed::from("command"), PhpMixed::from("self-update"))],
                None,
            )
            .unwrap(),
        ));
    let output_trait: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = output.clone();
    application.do_run(input, output_trait).unwrap();

    assert_eq!(
        "This instance of Composer does not have the self-update command.\n\
         This could be due to a number of reasons, such as Composer being installed as a system package on your OS, or Composer being installed as a package in the current project.\n",
        output.borrow().fetch().as_str()
    );
}

#[test]
fn test_process_isolation_works_multiple_times() {
    let _tear_down = TearDown;
    set_up();

    let application = ApplicationHandle::new("Composer".to_string(), "".to_string()).unwrap();
    let command: std::rc::Rc<std::cell::RefCell<dyn Command>> =
        std::rc::Rc::new(std::cell::RefCell::new(AboutCommand::new()));
    application.add(command).unwrap();

    let input1: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            ArrayInput::new(
                vec![(PhpMixed::from("command"), PhpMixed::from("about"))],
                None,
            )
            .unwrap(),
        ));
    let output1: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(BufferedOutput::new(None, false, None)),
    );
    assert_eq!(0, application.do_run(input1, output1).unwrap());

    let input2: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            ArrayInput::new(
                vec![(PhpMixed::from("command"), PhpMixed::from("about"))],
                None,
            )
            .unwrap(),
        ));
    let output2: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(BufferedOutput::new(None, false, None)),
    );
    assert_eq!(0, application.do_run(input2, output2).unwrap());
}

#[ignore = "Application::do_run registers the composer.json script as a command, a path that ends at a todo!() \
            (application.rs:2461, 'plugin: register reflection-instantiated command on Application::add'). With a \
            'scripts' key present, do_run panics there before getComposer is reached"]
#[test]
#[serial]
fn test_no_plugins_disables_plugins_when_script_commands_exist() {
    let _tear_down = TearDown;
    set_up();

    // PHP also calls setAutoExit(false)/setCatchErrors(false); both are Symfony base-Application
    // methods the external-package stub does not model, and neither affects the do_run path
    // exercised here (they only matter for run()/error catching), so only set_catch_exceptions
    // is mirrored.
    let _init = init_temp_composer(
        Some(&serde_json::json!({
            "scripts": {
                "my-script": "echo hello",
            },
        })),
        None,
        None,
        true,
    );

    let application = ApplicationHandle::new("Composer".to_string(), "".to_string()).unwrap();
    application.set_catch_exceptions(false);

    // Run list command with --no-plugins, this triggers script command registration which previously
    // created a Composer instance with plugins enabled regardless of the --no-plugins flag
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            ArrayInput::new(
                vec![
                    (PhpMixed::from("command"), PhpMixed::from("list")),
                    (PhpMixed::from("--no-plugins"), PhpMixed::from(true)),
                ],
                None,
            )
            .unwrap(),
        ));
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(BufferedOutput::new(None, false, None)),
    );
    application.do_run(input, output).unwrap();

    let composer = application.__get_composer(false, None, None).unwrap();
    assert!(
        composer.is_some(),
        "Composer instance should have been created during script command registration"
    );
    let composer = composer.unwrap();
    let composer = shirabe::composer::composer_full(&composer);
    assert!(
        composer
            .get_plugin_manager()
            .borrow()
            .are_plugins_disabled("local"),
        "Plugins should be disabled when --no-plugins is used"
    );
    assert!(
        composer
            .get_plugin_manager()
            .borrow()
            .are_plugins_disabled("global"),
        "Global plugins should be disabled when --no-plugins is used"
    );
}

#[ignore = "Application::do_run registers composer.json scripts as commands; that path ends at a todo!() \
            (application.rs:2461, 'plugin: register reflection-instantiated command on Application::add'). With a \
            'scripts' key present, do_run panics there before the script command executes"]
#[test]
#[serial]
fn test_script_command_takes_priority_over_abbreviated_builtin_command() {
    let _tear_down = TearDown;
    set_up();

    // PHP also calls setAutoExit(false)/setCatchErrors(false); both are Symfony base-Application
    // methods the external-package stub does not model, and neither affects the do_run path
    // exercised here (they only matter for run()/error catching), so only set_catch_exceptions
    // is mirrored.
    let _init = init_temp_composer(
        Some(&serde_json::json!({
            "scripts": {
                "check": "echo hello",
            },
        })),
        None,
        None,
        true,
    );

    let application = ApplicationHandle::new("Composer".to_string(), "".to_string()).unwrap();
    application.set_catch_exceptions(false);

    let app_output = std::rc::Rc::new(std::cell::RefCell::new(BufferedOutput::new(
        None, false, None,
    )));
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            ArrayInput::new(
                vec![(PhpMixed::from("command"), PhpMixed::from("check"))],
                None,
            )
            .unwrap(),
        ));
    let output_trait: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = app_output.clone();
    let exit_code = application.do_run(input, output_trait).unwrap();

    assert_eq!(0, exit_code, "Script command should have run successfully");
    assert!(
        app_output.borrow().fetch().contains("hello"),
        "The \"check\" script should have been executed instead of the check-platform-reqs command"
    );
}

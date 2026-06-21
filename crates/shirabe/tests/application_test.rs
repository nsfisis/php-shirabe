//! ref: composer/tests/Composer/Test/ApplicationTest.php

// These drive the console Application (doRun, command resolution, plugin disabling).
// The tests needing Application::setAutoExit/setCatchErrors or the initTempComposer
// helper, or a runtime define() of COMPOSER_DEV_WARNING_TIME, remain unportable.

use std::cell::RefCell;
use std::rc::Rc;

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

    todo!()
}

#[ignore]
#[test]
fn test_dev_warning_suppressed_for_self_update() {
    let _tear_down = TearDown;
    set_up();

    if Platform::is_windows() {
        // markTestSkipped('Does not run on windows')
        return;
    }

    let application = ApplicationHandle::new("Composer".to_string(), "".to_string()).unwrap();
    let command: Rc<RefCell<dyn Command>> = Rc::new(RefCell::new(SelfUpdateCommand::new()));
    application.add(command).unwrap();

    let output = Rc::new(RefCell::new(BufferedOutput::new(None, false, None)));
    let input: Rc<RefCell<dyn InputInterface>> = Rc::new(RefCell::new(
        ArrayInput::new(
            vec![(PhpMixed::from("command"), PhpMixed::from("self-update"))],
            None,
        )
        .unwrap(),
    ));
    let output_trait: Rc<RefCell<dyn OutputInterface>> = output.clone();
    application.do_run(input, output_trait).unwrap();

    assert_eq!(
        "This instance of Composer does not have the self-update command.\n\
         This could be due to a number of reasons, such as Composer being installed as a system package on your OS, or Composer being installed as a package in the current project.\n",
        output.borrow().fetch().as_str()
    );
}

#[ignore]
#[test]
fn test_process_isolation_works_multiple_times() {
    let _tear_down = TearDown;
    set_up();

    let application = ApplicationHandle::new("Composer".to_string(), "".to_string()).unwrap();
    let command: Rc<RefCell<dyn Command>> = Rc::new(RefCell::new(AboutCommand::new()));
    application.add(command).unwrap();

    let input1: Rc<RefCell<dyn InputInterface>> = Rc::new(RefCell::new(
        ArrayInput::new(
            vec![(PhpMixed::from("command"), PhpMixed::from("about"))],
            None,
        )
        .unwrap(),
    ));
    let output1: Rc<RefCell<dyn OutputInterface>> =
        Rc::new(RefCell::new(BufferedOutput::new(None, false, None)));
    assert_eq!(0, application.do_run(input1, output1).unwrap());

    let input2: Rc<RefCell<dyn InputInterface>> = Rc::new(RefCell::new(
        ArrayInput::new(
            vec![(PhpMixed::from("command"), PhpMixed::from("about"))],
            None,
        )
        .unwrap(),
    ));
    let output2: Rc<RefCell<dyn OutputInterface>> =
        Rc::new(RefCell::new(BufferedOutput::new(None, false, None)));
    assert_eq!(0, application.do_run(input2, output2).unwrap());
}

#[ignore = "Application::set_auto_exit / set_catch_errors and the initTempComposer test helper do not exist"]
#[test]
fn test_no_plugins_disables_plugins_when_script_commands_exist() {
    let _tear_down = TearDown;
    set_up();

    todo!()
}

#[ignore = "Application::set_auto_exit / set_catch_errors and the initTempComposer test helper do not exist"]
#[test]
fn test_script_command_takes_priority_over_abbreviated_builtin_command() {
    let _tear_down = TearDown;
    set_up();

    todo!()
}

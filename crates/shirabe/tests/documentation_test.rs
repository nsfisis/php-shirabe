//! ref: composer/tests/Composer/Test/DocumentationTest.php

use shirabe::console::application::ApplicationHandle;
use shirabe_external_packages::symfony::console::command::Command;
use shirabe_external_packages::symfony::console::descriptor::application_description::ApplicationDescription;
use std::cell::RefCell;
use std::rc::Rc;

fn get_command_name(command: &Rc<RefCell<dyn Command>>) -> String {
    let mut name = command.borrow().get_name().unwrap_or_default();
    for alias in command.borrow().get_aliases() {
        name = format!("{} / {}", name, alias);
    }

    name
}

fn provide_command_cases() -> Vec<Rc<RefCell<dyn Command>>> {
    let application = ApplicationHandle::new("Composer".to_string(), "".to_string()).unwrap();
    application.set_catch_exceptions(false);

    let mut description =
        ApplicationDescription::new(application.__base_application(), None, false);

    let mut commands = Vec::new();
    for command in description.get_commands().values() {
        if ["about", "completion", "list"]
            .contains(&command.borrow().get_name().as_deref().unwrap_or(""))
        {
            continue;
        }
        commands.push(command.clone());
    }

    commands
}

#[test]
fn test_command() {
    let doc_content = std::fs::read_to_string(format!(
        "{}/../../composer/doc/03-cli.md",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();

    for command in provide_command_cases() {
        assert!(
            // TODO: test description
            // TODO: test options
            doc_content.contains(&format!("\n## {}\n\n", get_command_name(&command))),
            "doc/03-cli.md does not contain a section for command \"{}\"",
            get_command_name(&command),
        );
    }
}

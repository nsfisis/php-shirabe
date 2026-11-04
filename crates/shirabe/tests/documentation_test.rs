//! ref: composer/tests/Composer/Test/DocumentationTest.php sha1:683180f3a77f548922fcfffb6ff2fc8f3437234c

use bstr::{BString, ByteSlice};
use shirabe::console::application::ApplicationHandle;
use shirabe_php_shim::bconcat;
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::descriptor::ApplicationDescription;

fn get_command_name(command: &std::rc::Rc<std::cell::RefCell<dyn Command>>) -> BString {
    let mut name = command.borrow().get_name().unwrap_or_default();
    for alias in command.borrow().get_aliases() {
        name = bconcat!(name, " / ", alias);
    }

    name
}

fn provide_command_cases() -> Vec<std::rc::Rc<std::cell::RefCell<dyn Command>>> {
    let application = ApplicationHandle::new(BString::from("Shirabe"), BString::from("")).unwrap();
    application.set_catch_exceptions(false);

    let mut description =
        ApplicationDescription::new(application.__base_application(), None, false);

    let mut commands = Vec::new();
    for command in description.get_commands().unwrap().values() {
        let name = command.borrow().get_name().unwrap_or_default();
        if ["about", "completion", "list"].iter().any(|n| name == *n) {
            continue;
        }
        commands.push(command.clone());
    }

    commands
}

#[test]
fn test_command() {
    let doc_content = bstr::BString::from(
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../composer/doc/03-cli.md"),
        )
        .unwrap(),
    );

    for command in provide_command_cases() {
        assert!(
            // TODO: test description
            // TODO: test options
            doc_content.as_bytes().contains_str(bconcat!(
                "\n## ",
                get_command_name(&command),
                "\n\n"
            )),
            "doc/03-cli.md does not contain a section for command \"{}\"",
            get_command_name(&command).display_lossy(),
        );
    }
}

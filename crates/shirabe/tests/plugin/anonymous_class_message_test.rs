//! Rendering of an exception message that names PHP anonymous classes. Symfony's
//! `Application::doRenderThrowable` rewrites each `Foo@anonymous\0<file>...` marker to
//! `Parent@anonymous` through the class table of the running PHP, which for Shirabe is the
//! worker's. Upstream has no test for this, so the PHP code is written here.

use crate::php_worker::lock_php_worker;
use bstr::{BStr, ByteSlice};
use shirabe::console::application::Application;
use shirabe::plugin::php_call::PhpCall;
use shirabe_php_rpc::PluginValue;
use shirabe_php_shim::path_to_bytes;
use shirabe_symfony_console::output::BufferedOutput;
use shirabe_symfony_console::output::output_interface::VERBOSITY_VERBOSE;

const THROWER: &[u8] = br#"<?php
$parent = new class extends \ArrayObject {};
$interface = new class implements \Countable {
    public function count(): int
    {
        return 0;
    }
};
$plain = new class {};
throw new \RuntimeException(implode("\n", [
    'parent=' . get_class($parent),
    'interface=' . get_class($interface),
    'plain=' . get_class($plain),
    'undeclared=' . 'Missing@anonymous' . "\0" . '/nowhere.php:1$0',
]));
"#;

#[test]
fn anonymous_class_names_in_a_message_are_rendered_by_their_parent() {
    let _worker = lock_php_worker();

    let dir = tempfile::TempDir::new().unwrap();
    let thrower = dir.path().join("thrower.php");
    std::fs::write(&thrower, THROWER).unwrap();
    let error = PhpCall::Function(BStr::new("__shirabe_require"))
        .run::<PluginValue>(vec![PluginValue::string(path_to_bytes(&thrower))])
        .expect_err("the required file throws");

    let application = Application::new("Shirabe".into(), "".into());
    let output = std::rc::Rc::new(std::cell::RefCell::new(BufferedOutput::new(
        None, false, None,
    )));
    application
        .do_render_throwable(&error, output.clone())
        .unwrap();
    let rendered = output.borrow().fetch();

    assert!(
        rendered.contains_str("parent=ArrayObject@anonymous "),
        "{rendered:?}"
    );
    assert!(
        rendered.contains_str("interface=Countable@anonymous "),
        "{rendered:?}"
    );
    assert!(
        rendered.contains_str("plain=class@anonymous "),
        "{rendered:?}"
    );
    // A name no class has is kept as is; the formatter writes its NUL byte as a backslash.
    assert!(
        rendered.contains_str(r"undeclared=Missing@anonymous\/nowhere.php:1$0"),
        "{rendered:?}"
    );
}

const ANONYMOUS_THROWER: &[u8] = br#"<?php
throw new class('boom') extends \RuntimeException {};
"#;

#[test]
fn an_anonymous_exception_class_is_titled_by_its_debug_type() {
    let _worker = lock_php_worker();

    let dir = tempfile::TempDir::new().unwrap();
    let thrower = dir.path().join("anonymous-thrower.php");
    std::fs::write(&thrower, ANONYMOUS_THROWER).unwrap();
    let error = PhpCall::Function(BStr::new("__shirabe_require"))
        .run::<PluginValue>(vec![PluginValue::string(path_to_bytes(&thrower))])
        .expect_err("the required file throws");

    let application = Application::new("Shirabe".into(), "".into());
    let output = std::rc::Rc::new(std::cell::RefCell::new(BufferedOutput::new(
        Some(VERBOSITY_VERBOSE),
        false,
        None,
    )));
    application
        .do_render_throwable(&error, output.clone())
        .unwrap();
    let rendered = output.borrow().fetch();

    assert!(
        rendered.contains_str("[RuntimeException@anonymous]"),
        "{rendered:?}"
    );
}

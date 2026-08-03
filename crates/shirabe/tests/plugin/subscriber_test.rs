//! Shirabe-specific integration tests for the subscriber plugin path: upstream Composer
//! has no test that exercises `addSubscriber`/`getSubscribedEvents` through a real plugin, so
//! these tests use a Shirabe-owned fixture (`fixtures/subscriber-v1`) instead of a ported one.

use crate::async_runtime::run;
use crate::plugin_installer_test::{lock_php_worker, new_installer, php_runtime_available, set_up};
use shirabe::installer::InstallerInterface;
use shirabe::package::PackageInterfaceHandle;
use shirabe::package::loader::{ArrayLoader, JsonLoader, JsonLoaderInput};

fn subscriber_fixture_package() -> PackageInterfaceHandle {
    let loader = JsonLoader::new(Box::new(ArrayLoader::new(None, false)));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/plugin/fixtures/subscriber-v1/composer.json");
    loader
        .load(JsonLoaderInput::String(
            path.canonicalize().unwrap().to_str().unwrap().to_string(),
        ))
        .unwrap()
}

/// Installs the subscriber fixture plugin: activate runs in the PHP child and
/// `addSubscriber` registers its listeners with the event dispatcher.
fn install_subscriber_plugin(set_up: &crate::plugin_installer_test::SetUp) {
    let installer = new_installer(set_up);
    set_up.pm.borrow_mut().load_installed_plugins().unwrap();
    run(installer.install(&set_up.repository, subscriber_fixture_package())).unwrap();
    assert_eq!("activate subscriber-v1\n", set_up.io.borrow().get_output());
}

fn dispatch(set_up: &crate::plugin_installer_test::SetUp, event_name: &str) -> i64 {
    let dispatcher = set_up.composer.borrow().get_event_dispatcher();
    let result = dispatcher.borrow_mut().dispatch(Some(event_name), None);
    result.unwrap()
}

#[test]
fn test_subscriber_listener_receives_event() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    install_subscriber_plugin(&set_up);

    // 'post-install-cmd' => 'onPostInstall' (bare method-name shape); the listener calls
    // $event->getName() back over RPC.
    let return_code = dispatch(&set_up, "post-install-cmd");

    assert_eq!(0, return_code);
    assert_eq!(
        "activate subscriber-v1\nsubscriber saw post-install-cmd\n",
        set_up.io.borrow().get_output()
    );
}

#[test]
fn test_subscriber_listeners_run_in_priority_order() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    install_subscriber_plugin(&set_up);

    // 'shirabe-priority-event' => [['early', 10], ['late', -10]] (multi-handler shape).
    let return_code = dispatch(&set_up, "shirabe-priority-event");

    assert_eq!(0, return_code);
    assert_eq!(
        "activate subscriber-v1\nearly listener\nlate listener\n",
        set_up.io.borrow().get_output()
    );
}

#[test]
fn test_subscriber_listener_returning_false_sets_return_code() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    install_subscriber_plugin(&set_up);

    // 'shirabe-false-event' => ['returnsFalse', 0] (method+priority shape); PHP maps a false
    // listener return to exit code 1.
    let return_code = dispatch(&set_up, "shirabe-false-event");

    assert_eq!(1, return_code);
    assert_eq!(
        "activate subscriber-v1\nfailing listener\n",
        set_up.io.borrow().get_output()
    );
}

#[test]
fn test_remove_plugin_removes_its_subscribed_listeners() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    install_subscriber_plugin(&set_up);

    assert_eq!(0, dispatch(&set_up, "post-install-cmd"));

    // The worker is shared across tests: when another test already defined the plugin class,
    // this install registered it under a `_composer_tmpN` rename (as upstream does).
    let plugin = set_up
        .pm
        .borrow()
        .get_plugins()
        .iter()
        .find(|p| {
            p.borrow()
                .get_class_name()
                .starts_with("Subscriber\\Plugin")
        })
        .expect("the subscriber plugin is registered")
        .clone();
    set_up.pm.borrow_mut().remove_plugin(&plugin).unwrap();

    // removePlugin removed the plugin's `[$subscriber, 'onPostInstall']` listener, so the
    // second dispatch produces no further output.
    let return_code = dispatch(&set_up, "post-install-cmd");

    assert_eq!(0, return_code);
    assert_eq!(
        "activate subscriber-v1\nsubscriber saw post-install-cmd\n",
        set_up.io.borrow().get_output()
    );
}

#[test]
fn test_unrelated_event_does_not_reach_the_subscriber() {
    if !php_runtime_available() {
        return;
    }
    let _worker = lock_php_worker();
    let set_up = set_up();
    install_subscriber_plugin(&set_up);

    let return_code = dispatch(&set_up, "shirabe-unrelated-event");

    assert_eq!(0, return_code);
    assert_eq!("activate subscriber-v1\n", set_up.io.borrow().get_output());
}

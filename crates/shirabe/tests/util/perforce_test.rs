//! ref: composer/tests/Composer/Test/Util/PerforceTest.php sha1:d37373d44bb83053fb3c561efdcf46db74af7a56

use crate::io_stub::IOStub;
use crate::process_executor_mock::{cmd, cmd_full, get_process_executor_mock};
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::io::{IOInterface, NullIO};
use shirabe::util::{Filesystem, MockHandler, Perforce, ProcessExecutor};
use shirabe_php_shim::{PHP_EOL, PhpMixed, bconcat, path_from_bytes, path_to_bytes, php_str};
use shirabe_symfony_console::input::InputValue;

const TEST_DEPOT: &BStr = php_str(b"depot");
const TEST_BRANCH: &BStr = php_str(b"branch");
const TEST_P4USER: &BStr = php_str(b"user");
const TEST_CLIENT_NAME: &BStr = php_str(b"TEST");
const TEST_PORT: &BStr = php_str(b"port");
const TEST_PATH: &BStr = php_str(b"path");

fn get_test_repo_config() -> IndexMap<BString, PhpMixed> {
    let mut config = IndexMap::new();
    config.insert("depot".into(), PhpMixed::string(bconcat!(TEST_DEPOT)));
    config.insert("branch".into(), PhpMixed::string(bconcat!(TEST_BRANCH)));
    config.insert("p4user".into(), PhpMixed::string(bconcat!(TEST_P4USER)));
    config.insert(
        "unique_perforce_client_name".into(),
        PhpMixed::string(bconcat!(TEST_CLIENT_NAME)),
    );
    config
}

fn create_new_perforce_with_windows_flag(flag: bool) -> Perforce {
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    Perforce::new(
        get_test_repo_config(),
        TEST_PORT.into(),
        path_from_bytes(TEST_PATH).to_path_buf(),
        process,
        flag,
        io,
    )
}

// Mirrors PHP `createNewPerforceWithWindowsFlag` but lets each test inject the mocked
// ProcessExecutor and IO it configured, matching the PHP `setUp` wiring.
fn create_perforce(
    flag: bool,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
) -> Perforce {
    Perforce::new(
        get_test_repo_config(),
        TEST_PORT.into(),
        path_from_bytes(TEST_PATH).to_path_buf(),
        process,
        flag,
        io,
    )
}

// The expected decoded composer.json, equivalent to PerforceTest::getComposerJson()
// after json_decode($json, true): an empty `psr-0` object decodes to an empty array.
fn expected_composer_information() -> IndexMap<BString, PhpMixed> {
    let mut autoload = IndexMap::new();
    autoload.insert("psr-0".into(), PhpMixed::Array(IndexMap::new()));

    let mut expected = IndexMap::new();
    expected.insert(
        "name".into(),
        PhpMixed::string(BString::from("test/perforce")),
    );
    expected.insert(
        "description".into(),
        PhpMixed::string(BString::from("Basic project for testing")),
    );
    expected.insert(
        "minimum-stability".into(),
        PhpMixed::string(BString::from("dev")),
    );
    expected.insert("autoload".into(), PhpMixed::Array(autoload));
    expected
}

// Valid composer.json content fed as p4 print stdout. Equivalent to
// PerforceTest::getComposerJson(): the exact formatting is irrelevant because the
// production code json_decodes it.
const COMPOSER_JSON: &BStr = php_str(r#"{"name":"test/perforce","description":"Basic project for testing","minimum-stability":"dev","autoload":{"psr-0":{}}}"#.as_bytes());

#[test]
fn test_get_client_without_stream() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    let client = perforce.get_client();

    let expected = "composer_perforce_TEST_depot";
    assert_eq!(expected, client);
}

#[test]
fn test_get_client_from_stream() {
    let mut perforce = create_new_perforce_with_windows_flag(true);
    perforce.set_stream("//depot/branch".into());

    let client = perforce.get_client();

    let expected = "composer_perforce_TEST_depot_branch";
    assert_eq!(expected, client);
}

#[test]
fn test_get_stream_without_stream() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    let stream = perforce.get_stream();
    assert_eq!("//depot", stream);
}

#[test]
fn test_get_stream_with_stream() {
    let mut perforce = create_new_perforce_with_windows_flag(true);
    perforce.set_stream("//depot/branch".into());

    let stream = perforce.get_stream();
    assert_eq!("//depot/branch", stream);
}

#[test]
fn test_get_stream_without_label_with_stream_without_label() {
    let perforce = create_new_perforce_with_windows_flag(true);

    let stream = perforce.get_stream_without_label("//depot/branch".into());
    assert_eq!("//depot/branch", stream);
}

#[test]
fn test_get_stream_without_label_with_stream_with_label() {
    let perforce = create_new_perforce_with_windows_flag(true);

    let stream = perforce.get_stream_without_label("//depot/branching@label".into());
    assert_eq!("//depot/branching", stream);
}

#[test]
fn test_get_client_spec() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    let client_spec = perforce.get_p4_client_spec();
    let expected = "path/composer_perforce_TEST_depot.p4.spec";
    assert_eq!(expected, BStr::new(path_to_bytes(&client_spec)));
}

#[test]
fn test_generate_p4_command() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    let p4_command = perforce.generate_p4_command(vec!["do".into(), "something".into()], true);
    let expected = vec![
        BString::from("p4"),
        BString::from("-u"),
        BString::from("user"),
        BString::from("-c"),
        BString::from("composer_perforce_TEST_depot"),
        BString::from("-p"),
        BString::from("port"),
        BString::from("do"),
        BString::from("something"),
    ];
    assert_eq!(expected, p4_command);
}

#[test]
fn test_query_p4_user_with_user_already_set() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    perforce.query_p4_user().unwrap();
    assert_eq!(Some(BString::from(TEST_P4USER)), perforce.get_user());
}

#[test]
fn test_query_p4_user_with_user_set_in_p4_variables_with_windows_os() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            "p4 set",
            0,
            bconcat!("P4USER=TEST_P4VARIABLE_USER", PHP_EOL).to_owned(),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_user(None);

    perforce.query_p4_user().unwrap();
    assert_eq!(
        Some(BString::from("TEST_P4VARIABLE_USER")),
        perforce.get_user()
    );
}

#[test]
fn test_query_p4_user_with_user_set_in_p4_variables_not_windows_os() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            "echo $P4USER",
            0,
            bconcat!("TEST_P4VARIABLE_USER", PHP_EOL).to_owned(),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(false, process, io);
    perforce.set_user(None);

    perforce.query_p4_user().unwrap();
    assert_eq!(
        Some(BString::from("TEST_P4VARIABLE_USER")),
        perforce.get_user()
    );
}

#[test]
fn test_query_p4_user_queries_for_user() {
    // Non-strict empty process mock: the p4-variable lookup returns empty so the
    // code falls through to io->ask().
    let (process, _guard) = get_process_executor_mock(vec![], false, MockHandler::default());
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            IOStub::new().with_ask(InputValue::String(BString::from("TEST_QUERY_USER"))),
        ));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_user(None);

    perforce.query_p4_user().unwrap();
    assert_eq!(Some(BString::from("TEST_QUERY_USER")), perforce.get_user());
}

#[test]
fn test_query_p4_user_stores_response_to_query_for_user_with_windows() {
    let expected_command = bconcat!("p4 set P4USER=", ProcessExecutor::escape("TEST_QUERY_USER"));
    let (process, _guard) = get_process_executor_mock(
        vec![cmd("p4 set"), cmd(expected_command.as_bstr())],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            IOStub::new().with_ask(InputValue::String(BString::from("TEST_QUERY_USER"))),
        ));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_user(None);

    perforce.query_p4_user().unwrap();
}

#[test]
fn test_query_p4_user_stores_response_to_query_for_user_without_windows() {
    let expected_command = bconcat!("export P4USER=", ProcessExecutor::escape("TEST_QUERY_USER"));
    let (process, _guard) = get_process_executor_mock(
        vec![cmd("echo $P4USER"), cmd(expected_command.as_bstr())],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            IOStub::new().with_ask(InputValue::String(BString::from("TEST_QUERY_USER"))),
        ));
    let mut perforce = create_perforce(false, process, io);
    perforce.set_user(None);

    perforce.query_p4_user().unwrap();
}

#[test]
fn test_query_p4_user_escapes_injection_on_windows() {
    let expected_command = bconcat!("p4 set P4USER=", ProcessExecutor::escape("foo && calc.exe"));
    let (process, _guard) = get_process_executor_mock(
        vec![cmd("p4 set"), cmd(expected_command.as_bstr())],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            IOStub::new().with_ask(InputValue::String(BString::from("foo && calc.exe"))),
        ));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_user(None);

    perforce.query_p4_user().unwrap();
}

#[test]
fn test_query_p4_user_escapes_injection_on_unix() {
    let expected_command = bconcat!("export P4USER=", ProcessExecutor::escape("foo; id"));
    let (process, _guard) = get_process_executor_mock(
        vec![cmd("echo $P4USER"), cmd(expected_command.as_bstr())],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            IOStub::new().with_ask(InputValue::String(BString::from("foo; id"))),
        ));
    let mut perforce = create_perforce(false, process, io);
    perforce.set_user(None);

    perforce.query_p4_user().unwrap();
}

#[test]
fn test_query_p4_password_with_password_already_set() {
    let mut repo_config = IndexMap::new();
    repo_config.insert("depot".into(), PhpMixed::string(BString::from("depot")));
    repo_config.insert("branch".into(), PhpMixed::string(BString::from("branch")));
    repo_config.insert("p4user".into(), PhpMixed::string(BString::from("user")));
    repo_config.insert(
        "p4password".into(),
        PhpMixed::string(BString::from("TEST_PASSWORD")),
    );
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = Perforce::new(
        repo_config,
        "port".into(),
        "path".into(),
        process,
        false,
        io,
    );

    let password = perforce.query_p4_password().unwrap();
    assert_eq!(Some(BString::from("TEST_PASSWORD")), password);
}

#[test]
fn test_query_p4_password_with_password_set_in_p4_variables_with_windows_os() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            "p4 set",
            0,
            bconcat!("P4PASSWD=TEST_P4VARIABLE_PASSWORD", PHP_EOL).to_owned(),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    let password = perforce.query_p4_password().unwrap();
    assert_eq!(Some(BString::from("TEST_P4VARIABLE_PASSWORD")), password);
}

#[test]
fn test_query_p4_password_with_password_set_in_p4_variables_not_windows_os() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            "echo $P4PASSWD",
            0,
            bconcat!("TEST_P4VARIABLE_PASSWORD", PHP_EOL).to_owned(),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(false, process, io);

    let password = perforce.query_p4_password().unwrap();
    assert_eq!(Some(BString::from("TEST_P4VARIABLE_PASSWORD")), password);
}

#[test]
fn test_query_p4_password_queries_for_password() {
    // Non-strict empty process mock: the p4-variable lookup returns empty so the
    // code falls through to io->askAndHideAnswer().
    let (process, _guard) = get_process_executor_mock(vec![], false, MockHandler::default());
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            IOStub::new().with_ask_and_hide_answer(Some(BString::from("TEST_QUERY_PASSWORD"))),
        ));
    let mut perforce = create_perforce(true, process, io);

    let password = perforce.query_p4_password().unwrap();
    assert_eq!(Some(BString::from("TEST_QUERY_PASSWORD")), password);
}

#[test]
fn test_is_logged_in() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd(vec!["p4", "-u", "user", "-p", "port", "login", "-s"])],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    perforce.is_logged_in().unwrap();
}

#[test]
fn test_get_branches_with_stream() {
    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd_full(
                vec![
                    "p4",
                    "-u",
                    "user",
                    "-c",
                    "composer_perforce_TEST_depot_branch",
                    "-p",
                    "port",
                    "streams",
                    "//depot/...",
                ],
                0,
                bconcat!("Stream //depot/branch mainline none 'branch'", PHP_EOL).to_owned(),
                "",
            ),
            cmd_full(
                vec![
                    "p4",
                    "-u",
                    "user",
                    "-p",
                    "port",
                    "changes",
                    "//depot/branch/...",
                ],
                0,
                "Change 1234 on 2014/03/19 by Clark.Stuth@Clark.Stuth_test_client 'test changelist'",
                "",
            ),
        ],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_stream("//depot/branch".into());

    let branches = perforce.get_branches();
    assert_eq!("//depot/branch@1234", branches[B("master")]);
}

#[test]
fn test_get_branches_without_stream() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec!["p4", "-u", "user", "-p", "port", "changes", "//depot/..."],
            0,
            "Change 5678 on 2014/03/19 by Clark.Stuth@Clark.Stuth_test_client 'test changelist'",
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    let branches = perforce.get_branches();
    assert_eq!("//depot@5678", branches[B("master")]);
}

#[test]
fn test_get_tags_without_stream() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec![
                "p4",
                "-u",
                "user",
                "-c",
                "composer_perforce_TEST_depot",
                "-p",
                "port",
                "labels",
            ],
            0,
            bconcat!(
                "Label 0.0.1 2013/07/31 'First Label!'",
                PHP_EOL,
                "Label 0.0.2 2013/08/01 'Second Label!'",
                PHP_EOL
            )
            .to_owned(),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    let tags = perforce.get_tags();
    assert_eq!("//depot@0.0.1", tags[B("0.0.1")]);
    assert_eq!("//depot@0.0.2", tags[B("0.0.2")]);
}

#[test]
fn test_get_tags_with_stream() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec![
                "p4",
                "-u",
                "user",
                "-c",
                "composer_perforce_TEST_depot_branch",
                "-p",
                "port",
                "labels",
            ],
            0,
            bconcat!(
                "Label 0.0.1 2013/07/31 'First Label!'",
                PHP_EOL,
                "Label 0.0.2 2013/08/01 'Second Label!'",
                PHP_EOL
            )
            .to_owned(),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_stream("//depot/branch".into());

    let tags = perforce.get_tags();
    assert_eq!("//depot/branch@0.0.1", tags[B("0.0.1")]);
    assert_eq!("//depot/branch@0.0.2", tags[B("0.0.2")]);
}

#[test]
fn test_check_stream_without_stream() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    let result = perforce.check_stream();
    assert!(!result);
    assert!(!perforce.is_stream());
}

#[test]
fn test_check_stream_with_stream() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec!["p4", "-u", "user", "-p", "port", "depots"],
            0,
            "Depot depot 2013/06/25 stream /p4/1/depots/depot/... 'Created by Me'",
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    let result = perforce.check_stream();
    assert!(result);
    assert!(perforce.is_stream());
}

#[test]
fn test_get_composer_information_without_label_without_stream() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec![
                "p4",
                "-u",
                "user",
                "-c",
                "composer_perforce_TEST_depot",
                "-p",
                "port",
                "print",
                "//depot/composer.json",
            ],
            0,
            COMPOSER_JSON,
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    let result = perforce.get_composer_information("//depot".into()).unwrap();
    assert_eq!(Some(expected_composer_information()), result);
}

#[test]
fn test_get_composer_information_with_label_without_stream() {
    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd_full(
                vec![
                    "p4",
                    "-u",
                    "user",
                    "-p",
                    "port",
                    "files",
                    "//depot/composer.json@0.0.1",
                ],
                0,
                "//depot/composer.json#1 - branch change 10001 (text)",
                "",
            ),
            cmd_full(
                vec![
                    "p4",
                    "-u",
                    "user",
                    "-c",
                    "composer_perforce_TEST_depot",
                    "-p",
                    "port",
                    "print",
                    "//depot/composer.json@10001",
                ],
                0,
                COMPOSER_JSON,
                "",
            ),
        ],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    let result = perforce
        .get_composer_information("//depot@0.0.1".into())
        .unwrap();
    assert_eq!(Some(expected_composer_information()), result);
}

#[test]
fn test_get_composer_information_without_label_with_stream() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec![
                "p4",
                "-u",
                "user",
                "-c",
                "composer_perforce_TEST_depot_branch",
                "-p",
                "port",
                "print",
                "//depot/branch/composer.json",
            ],
            0,
            COMPOSER_JSON,
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_stream("//depot/branch".into());

    let result = perforce
        .get_composer_information("//depot/branch".into())
        .unwrap();
    assert_eq!(Some(expected_composer_information()), result);
}

#[test]
fn test_get_composer_information_with_label_with_stream() {
    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd_full(
                vec![
                    "p4",
                    "-u",
                    "user",
                    "-p",
                    "port",
                    "files",
                    "//depot/branch/composer.json@0.0.1",
                ],
                0,
                "//depot/composer.json#1 - branch change 10001 (text)",
                "",
            ),
            cmd_full(
                vec![
                    "p4",
                    "-u",
                    "user",
                    "-c",
                    "composer_perforce_TEST_depot_branch",
                    "-p",
                    "port",
                    "print",
                    "//depot/branch/composer.json@10001",
                ],
                0,
                COMPOSER_JSON,
                "",
            ),
        ],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_stream("//depot/branch".into());

    let result = perforce
        .get_composer_information("//depot/branch@0.0.1".into())
        .unwrap();
    assert_eq!(Some(expected_composer_information()), result);
}

#[test]
#[serial]
fn test_sync_code_base_without_stream() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd(vec![
            "p4",
            "-u",
            "user",
            "-c",
            "composer_perforce_TEST_depot",
            "-p",
            "port",
            "sync",
            "-f",
            "@label",
        ])],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    perforce.sync_code_base(Some("label".into())).unwrap();
}

#[test]
#[serial]
fn test_sync_code_base_with_stream() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd(vec![
            "p4",
            "-u",
            "user",
            "-c",
            "composer_perforce_TEST_depot_branch",
            "-p",
            "port",
            "sync",
            "-f",
            "@label",
        ])],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_stream("//depot/branch".into());

    perforce.sync_code_base(Some("label".into())).unwrap();
}

#[test]
fn test_check_server_exists() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd(vec![
            "p4",
            "-p",
            "perforce.does.exist:port",
            "info",
            "-s",
        ])],
        true,
        MockHandler::default(),
    );

    let result =
        Perforce::check_server_exists("perforce.does.exist:port".into(), &mut process.borrow_mut())
            .unwrap();
    assert!(result);
}

#[test]
fn test_check_server_client_error() {
    // PHP mocks ProcessExecutor::execute -> 127. The mock harness returns the
    // configured return code for the matched command.
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec!["p4", "-p", "perforce.does.exist:port", "info", "-s"],
            127,
            "",
            "",
        )],
        true,
        MockHandler::default(),
    );

    let result =
        Perforce::check_server_exists("perforce.does.exist:port".into(), &mut process.borrow_mut())
            .unwrap();
    assert!(!result);
}

#[test]
fn test_cleanup_client_spec_should_delete_client() {
    let test_client = BStr::new("composer_perforce_TEST_depot");
    let (process, _guard) = get_process_executor_mock(
        vec![cmd(vec![
            BStr::new("p4"),
            BStr::new("-u"),
            TEST_P4USER,
            BStr::new("-p"),
            TEST_PORT,
            BStr::new("client"),
            BStr::new("-d"),
            test_client,
        ])],
        true,
        MockHandler::default(),
    );
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process.clone(), io);

    let fs = std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(Some(process))));
    perforce.set_filesystem(fs);

    perforce.cleanup_client_spec();
}

//! ref: composer/tests/Composer/Test/Util/PerforceTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use serial_test::serial;
use shirabe::io::{IOInterface, NullIO};
use shirabe::util::Perforce;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::process_executor::{MockHandler, ProcessExecutor};
use shirabe_php_shim::PhpMixed;

use crate::io_stub::IOStub;
use crate::process_executor_mock::{cmd, cmd_full, get_process_executor_mock};

const TEST_DEPOT: &str = "depot";
const TEST_BRANCH: &str = "branch";
const TEST_P4USER: &str = "user";
const TEST_CLIENT_NAME: &str = "TEST";
const TEST_PORT: &str = "port";
const TEST_PATH: &str = "path";

fn get_test_repo_config() -> IndexMap<String, PhpMixed> {
    let mut config = IndexMap::new();
    config.insert(
        "depot".to_string(),
        PhpMixed::String(TEST_DEPOT.to_string()),
    );
    config.insert(
        "branch".to_string(),
        PhpMixed::String(TEST_BRANCH.to_string()),
    );
    config.insert(
        "p4user".to_string(),
        PhpMixed::String(TEST_P4USER.to_string()),
    );
    config.insert(
        "unique_perforce_client_name".to_string(),
        PhpMixed::String(TEST_CLIENT_NAME.to_string()),
    );
    config
}

fn create_new_perforce_with_windows_flag(flag: bool) -> Perforce {
    let process = Rc::new(RefCell::new(ProcessExecutor::new(None)));
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    Perforce::new(
        get_test_repo_config(),
        TEST_PORT.to_string(),
        TEST_PATH.to_string(),
        process,
        flag,
        io,
    )
}

// Mirrors PHP `createNewPerforceWithWindowsFlag` but lets each test inject the mocked
// ProcessExecutor and IO it configured, matching the PHP `setUp` wiring.
fn create_perforce(
    flag: bool,
    process: Rc<RefCell<ProcessExecutor>>,
    io: Rc<RefCell<dyn IOInterface>>,
) -> Perforce {
    Perforce::new(
        get_test_repo_config(),
        TEST_PORT.to_string(),
        TEST_PATH.to_string(),
        process,
        flag,
        io,
    )
}

// The expected decoded composer.json, equivalent to PerforceTest::getComposerJson()
// after json_decode($json, true): an empty `psr-0` object decodes to an empty array.
fn expected_composer_information() -> IndexMap<String, PhpMixed> {
    let mut autoload = IndexMap::new();
    autoload.insert("psr-0".to_string(), PhpMixed::Array(IndexMap::new()));

    let mut expected = IndexMap::new();
    expected.insert(
        "name".to_string(),
        PhpMixed::String("test/perforce".to_string()),
    );
    expected.insert(
        "description".to_string(),
        PhpMixed::String("Basic project for testing".to_string()),
    );
    expected.insert(
        "minimum-stability".to_string(),
        PhpMixed::String("dev".to_string()),
    );
    expected.insert("autoload".to_string(), PhpMixed::Array(autoload));
    expected
}

// Valid composer.json content fed as p4 print stdout. Equivalent to
// PerforceTest::getComposerJson(): the exact formatting is irrelevant because the
// production code json_decodes it.
const COMPOSER_JSON: &str = r#"{"name":"test/perforce","description":"Basic project for testing","minimum-stability":"dev","autoload":{"psr-0":{}}}"#;

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
    perforce.set_stream("//depot/branch");

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
    perforce.set_stream("//depot/branch");

    let stream = perforce.get_stream();
    assert_eq!("//depot/branch", stream);
}

#[test]
fn test_get_stream_without_label_with_stream_without_label() {
    let perforce = create_new_perforce_with_windows_flag(true);

    let stream = perforce.get_stream_without_label("//depot/branch");
    assert_eq!("//depot/branch", stream);
}

#[test]
fn test_get_stream_without_label_with_stream_with_label() {
    let perforce = create_new_perforce_with_windows_flag(true);

    let stream = perforce.get_stream_without_label("//depot/branching@label");
    assert_eq!("//depot/branching", stream);
}

#[test]
fn test_get_client_spec() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    let client_spec = perforce.get_p4_client_spec();
    let expected = "path/composer_perforce_TEST_depot.p4.spec";
    assert_eq!(expected, client_spec);
}

#[test]
fn test_generate_p4_command() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    let p4_command =
        perforce.generate_p4_command(vec!["do".to_string(), "something".to_string()], true);
    let expected = vec![
        "p4".to_string(),
        "-u".to_string(),
        "user".to_string(),
        "-c".to_string(),
        "composer_perforce_TEST_depot".to_string(),
        "-p".to_string(),
        "port".to_string(),
        "do".to_string(),
        "something".to_string(),
    ];
    assert_eq!(expected, p4_command);
}

#[test]
fn test_query_p4_user_with_user_already_set() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    perforce.query_p4_user();
    assert_eq!(Some(TEST_P4USER.to_string()), perforce.get_user());
}

#[test]
fn test_query_p4_user_with_user_set_in_p4_variables_with_windows_os() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec!["p4 set"],
            0,
            format!("P4USER=TEST_P4VARIABLE_USER{}", shirabe_php_shim::PHP_EOL),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_user(None);

    perforce.query_p4_user();
    assert_eq!(
        Some("TEST_P4VARIABLE_USER".to_string()),
        perforce.get_user()
    );
}

#[test]
fn test_query_p4_user_with_user_set_in_p4_variables_not_windows_os() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec!["echo $P4USER"],
            0,
            format!("TEST_P4VARIABLE_USER{}", shirabe_php_shim::PHP_EOL),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(false, process, io);
    perforce.set_user(None);

    perforce.query_p4_user();
    assert_eq!(
        Some("TEST_P4VARIABLE_USER".to_string()),
        perforce.get_user()
    );
}

#[test]
fn test_query_p4_user_queries_for_user() {
    // Non-strict empty process mock: the p4-variable lookup returns empty so the
    // code falls through to io->ask().
    let (process, _guard) = get_process_executor_mock(vec![], false, MockHandler::default());
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(
        IOStub::new().with_ask(PhpMixed::String("TEST_QUERY_USER".to_string())),
    ));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_user(None);

    perforce.query_p4_user();
    assert_eq!(Some("TEST_QUERY_USER".to_string()), perforce.get_user());
}

#[test]
fn test_query_p4_user_stores_response_to_query_for_user_with_windows() {
    let expected_command = format!(
        "p4 set P4USER={}",
        ProcessExecutor::escape("TEST_QUERY_USER")
    );
    let (process, _guard) = get_process_executor_mock(
        vec![cmd(vec!["p4 set"]), cmd(vec![expected_command.as_str()])],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(
        IOStub::new().with_ask(PhpMixed::String("TEST_QUERY_USER".to_string())),
    ));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_user(None);

    perforce.query_p4_user();
}

#[test]
fn test_query_p4_user_stores_response_to_query_for_user_without_windows() {
    let expected_command = format!(
        "export P4USER={}",
        ProcessExecutor::escape("TEST_QUERY_USER")
    );
    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(vec!["echo $P4USER"]),
            cmd(vec![expected_command.as_str()]),
        ],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(
        IOStub::new().with_ask(PhpMixed::String("TEST_QUERY_USER".to_string())),
    ));
    let mut perforce = create_perforce(false, process, io);
    perforce.set_user(None);

    perforce.query_p4_user();
}

#[test]
fn test_query_p4_user_escapes_injection_on_windows() {
    let expected_command = format!(
        "p4 set P4USER={}",
        ProcessExecutor::escape("foo && calc.exe")
    );
    let (process, _guard) = get_process_executor_mock(
        vec![cmd(vec!["p4 set"]), cmd(vec![expected_command.as_str()])],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(
        IOStub::new().with_ask(PhpMixed::String("foo && calc.exe".to_string())),
    ));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_user(None);

    perforce.query_p4_user();
}

#[test]
fn test_query_p4_user_escapes_injection_on_unix() {
    let expected_command = format!("export P4USER={}", ProcessExecutor::escape("foo; id"));
    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(vec!["echo $P4USER"]),
            cmd(vec![expected_command.as_str()]),
        ],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(
        IOStub::new().with_ask(PhpMixed::String("foo; id".to_string())),
    ));
    let mut perforce = create_perforce(false, process, io);
    perforce.set_user(None);

    perforce.query_p4_user();
}

#[test]
fn test_query_p4_password_with_password_already_set() {
    let mut repo_config = IndexMap::new();
    repo_config.insert("depot".to_string(), PhpMixed::String("depot".to_string()));
    repo_config.insert("branch".to_string(), PhpMixed::String("branch".to_string()));
    repo_config.insert("p4user".to_string(), PhpMixed::String("user".to_string()));
    repo_config.insert(
        "p4password".to_string(),
        PhpMixed::String("TEST_PASSWORD".to_string()),
    );
    let process = Rc::new(RefCell::new(ProcessExecutor::new(None)));
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = Perforce::new(
        repo_config,
        "port".to_string(),
        "path".to_string(),
        process,
        false,
        io,
    );

    let password = perforce.query_p4_password();
    assert_eq!(Some("TEST_PASSWORD".to_string()), password);
}

#[test]
fn test_query_p4_password_with_password_set_in_p4_variables_with_windows_os() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec!["p4 set"],
            0,
            format!(
                "P4PASSWD=TEST_P4VARIABLE_PASSWORD{}",
                shirabe_php_shim::PHP_EOL
            ),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    let password = perforce.query_p4_password();
    assert_eq!(Some("TEST_P4VARIABLE_PASSWORD".to_string()), password);
}

#[test]
fn test_query_p4_password_with_password_set_in_p4_variables_not_windows_os() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd_full(
            vec!["echo $P4PASSWD"],
            0,
            format!("TEST_P4VARIABLE_PASSWORD{}", shirabe_php_shim::PHP_EOL),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(false, process, io);

    let password = perforce.query_p4_password();
    assert_eq!(Some("TEST_P4VARIABLE_PASSWORD".to_string()), password);
}

#[test]
fn test_query_p4_password_queries_for_password() {
    // Non-strict empty process mock: the p4-variable lookup returns empty so the
    // code falls through to io->askAndHideAnswer().
    let (process, _guard) = get_process_executor_mock(vec![], false, MockHandler::default());
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(
        IOStub::new().with_ask_and_hide_answer(Some("TEST_QUERY_PASSWORD".to_string())),
    ));
    let mut perforce = create_perforce(true, process, io);

    let password = perforce.query_p4_password();
    assert_eq!(Some("TEST_QUERY_PASSWORD".to_string()), password);
}

#[test]
fn test_is_logged_in() {
    let (process, _guard) = get_process_executor_mock(
        vec![cmd(vec!["p4", "-u", "user", "-p", "port", "login", "-s"])],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
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
                format!(
                    "Stream //depot/branch mainline none 'branch'{}",
                    shirabe_php_shim::PHP_EOL
                ),
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
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_stream("//depot/branch");

    let branches = perforce.get_branches();
    assert_eq!("//depot/branch@1234", branches["master"]);
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
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    let branches = perforce.get_branches();
    assert_eq!("//depot@5678", branches["master"]);
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
            format!(
                "Label 0.0.1 2013/07/31 'First Label!'{}Label 0.0.2 2013/08/01 'Second Label!'{}",
                shirabe_php_shim::PHP_EOL,
                shirabe_php_shim::PHP_EOL
            ),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    let tags = perforce.get_tags();
    assert_eq!("//depot@0.0.1", tags["0.0.1"]);
    assert_eq!("//depot@0.0.2", tags["0.0.2"]);
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
            format!(
                "Label 0.0.1 2013/07/31 'First Label!'{}Label 0.0.2 2013/08/01 'Second Label!'{}",
                shirabe_php_shim::PHP_EOL,
                shirabe_php_shim::PHP_EOL
            ),
            "",
        )],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_stream("//depot/branch");

    let tags = perforce.get_tags();
    assert_eq!("//depot/branch@0.0.1", tags["0.0.1"]);
    assert_eq!("//depot/branch@0.0.2", tags["0.0.2"]);
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
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
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
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    let result = perforce.get_composer_information("//depot").unwrap();
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
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    let result = perforce.get_composer_information("//depot@0.0.1").unwrap();
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
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_stream("//depot/branch");

    let result = perforce.get_composer_information("//depot/branch").unwrap();
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
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_stream("//depot/branch");

    let result = perforce
        .get_composer_information("//depot/branch@0.0.1")
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
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);

    perforce.sync_code_base(Some("label")).unwrap();
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
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process, io);
    perforce.set_stream("//depot/branch");

    perforce.sync_code_base(Some("label")).unwrap();
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
        Perforce::check_server_exists("perforce.does.exist:port", &mut process.borrow_mut());
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
        Perforce::check_server_exists("perforce.does.exist:port", &mut process.borrow_mut());
    assert!(!result);
}

#[test]
fn test_cleanup_client_spec_should_delete_client() {
    let test_client = "composer_perforce_TEST_depot";
    let (process, _guard) = get_process_executor_mock(
        vec![cmd(vec![
            "p4",
            "-u",
            TEST_P4USER,
            "-p",
            TEST_PORT,
            "client",
            "-d",
            test_client,
        ])],
        true,
        MockHandler::default(),
    );
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut perforce = create_perforce(true, process.clone(), io);

    let fs = Rc::new(RefCell::new(Filesystem::new(Some(process))));
    perforce.set_filesystem(fs);

    perforce.cleanup_client_spec();
}

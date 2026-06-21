//! ref: composer/tests/Composer/Test/Util/PerforceTest.php

// These mock IO and a ProcessExecutor to drive Perforce client/stream/command behaviour;
// mocking is not available here.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::io::{IOInterface, NullIO};
use shirabe::util::{Perforce, ProcessExecutor};
use shirabe_php_shim::PhpMixed;

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

#[allow(dead_code)]
fn set_up() {
    // Builds mocked ProcessExecutor/IO, the test repo config, and a Windows-flagged Perforce;
    // mocking is not available.
    todo!()
}

#[ignore]
#[test]
fn test_get_client_without_stream() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    let client = perforce.get_client();

    let expected = "composer_perforce_TEST_depot";
    assert_eq!(expected, client);
}

#[ignore]
#[test]
fn test_get_client_from_stream() {
    let mut perforce = create_new_perforce_with_windows_flag(true);
    perforce.set_stream("//depot/branch");

    let client = perforce.get_client();

    let expected = "composer_perforce_TEST_depot_branch";
    assert_eq!(expected, client);
}

#[ignore]
#[test]
fn test_get_stream_without_stream() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    let stream = perforce.get_stream();
    assert_eq!("//depot", stream);
}

#[ignore]
#[test]
fn test_get_stream_with_stream() {
    let mut perforce = create_new_perforce_with_windows_flag(true);
    perforce.set_stream("//depot/branch");

    let stream = perforce.get_stream();
    assert_eq!("//depot/branch", stream);
}

#[ignore]
#[test]
fn test_get_stream_without_label_with_stream_without_label() {
    let perforce = create_new_perforce_with_windows_flag(true);

    let stream = perforce.get_stream_without_label("//depot/branch");
    assert_eq!("//depot/branch", stream);
}

#[ignore]
#[test]
fn test_get_stream_without_label_with_stream_with_label() {
    let perforce = create_new_perforce_with_windows_flag(true);

    let stream = perforce.get_stream_without_label("//depot/branching@label");
    assert_eq!("//depot/branching", stream);
}

#[ignore]
#[test]
fn test_get_client_spec() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    let client_spec = perforce.get_p4_client_spec();
    let expected = "path/composer_perforce_TEST_depot.p4.spec";
    assert_eq!(expected, client_spec);
}

#[ignore]
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

#[ignore]
#[test]
fn test_query_p4_user_with_user_already_set() {
    let mut perforce = create_new_perforce_with_windows_flag(true);

    perforce.query_p4_user();
    assert_eq!(Some(TEST_P4USER.to_string()), perforce.get_user());
}

#[test]
#[ignore = "requires getProcessExecutorMock with expects() command stdout stubbing (p4 set => P4USER=...); no mocking infrastructure exists"]
fn test_query_p4_user_with_user_set_in_p4_variables_with_windows_os() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock with expects() command stdout stubbing (echo $P4USER => ...); no mocking infrastructure exists"]
fn test_query_p4_user_with_user_set_in_p4_variables_not_windows_os() {
    todo!()
}

#[test]
#[ignore = "requires mocked IOInterface ask()->willReturn and getProcessExecutorMock; no mocking infrastructure exists"]
fn test_query_p4_user_queries_for_user() {
    todo!()
}

#[test]
#[ignore = "requires mocked IOInterface ask()->willReturn and getProcessExecutorMock expects() command verification; no mocking infrastructure exists"]
fn test_query_p4_user_stores_response_to_query_for_user_with_windows() {
    todo!()
}

#[test]
#[ignore = "requires mocked IOInterface ask()->willReturn and getProcessExecutorMock expects() command verification; no mocking infrastructure exists"]
fn test_query_p4_user_stores_response_to_query_for_user_without_windows() {
    todo!()
}

#[test]
#[ignore = "requires mocked IOInterface ask()->willReturn and getProcessExecutorMock expects() command verification; no mocking infrastructure exists"]
fn test_query_p4_user_escapes_injection_on_windows() {
    todo!()
}

#[test]
#[ignore = "requires mocked IOInterface ask()->willReturn and getProcessExecutorMock expects() command verification; no mocking infrastructure exists"]
fn test_query_p4_user_escapes_injection_on_unix() {
    todo!()
}

#[ignore]
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
#[ignore = "requires getProcessExecutorMock with expects() command stdout stubbing (p4 set => P4PASSWD=...); no mocking infrastructure exists"]
fn test_query_p4_password_with_password_set_in_p4_variables_with_windows_os() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock with expects() command stdout stubbing (echo $P4PASSWD => ...); no mocking infrastructure exists"]
fn test_query_p4_password_with_password_set_in_p4_variables_not_windows_os() {
    todo!()
}

#[test]
#[ignore = "requires mocked IOInterface askAndHideAnswer()->willReturn; no mocking infrastructure exists"]
fn test_query_p4_password_queries_for_password() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command verification for the p4 client spec write; no mocking infrastructure exists"]
fn test_write_p4_client_spec_without_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command verification for the p4 client spec write; no mocking infrastructure exists"]
fn test_write_p4_client_spec_with_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 login -s; no mocking infrastructure exists"]
fn test_is_logged_in() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 streams; no mocking infrastructure exists"]
fn test_get_branches_with_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 changes; no mocking infrastructure exists"]
fn test_get_branches_without_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 changes; no mocking infrastructure exists"]
fn test_get_tags_without_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 changes; no mocking infrastructure exists"]
fn test_get_tags_with_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 streams; no mocking infrastructure exists"]
fn test_check_stream_without_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 streams; no mocking infrastructure exists"]
fn test_check_stream_with_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 print composer.json; no mocking infrastructure exists"]
fn test_get_composer_information_without_label_without_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 print composer.json; no mocking infrastructure exists"]
fn test_get_composer_information_with_label_without_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 print composer.json; no mocking infrastructure exists"]
fn test_get_composer_information_without_label_with_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 print composer.json; no mocking infrastructure exists"]
fn test_get_composer_information_with_label_with_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command verification for p4 sync; no mocking infrastructure exists"]
fn test_sync_code_base_without_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command verification for p4 sync; no mocking infrastructure exists"]
fn test_sync_code_base_with_stream() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 info -s; no mocking infrastructure exists"]
fn test_check_server_exists() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command stdout stubbing for p4 info -s; no mocking infrastructure exists"]
fn test_check_server_client_error() {
    todo!()
}

#[test]
#[ignore = "requires getProcessExecutorMock expects() command verification for p4 client -d; no mocking infrastructure exists"]
fn test_cleanup_client_spec_should_delete_client() {
    todo!()
}

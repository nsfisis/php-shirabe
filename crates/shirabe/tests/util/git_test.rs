//! ref: composer/tests/Composer/Test/Util/GitTest.php

use crate::config_stub::ConfigStubBuilder;
use crate::http_downloader_mock::{expect_full, get_http_downloader_mock};
use crate::io_stub::IOStub;
use crate::process_executor_mock::{cmd, cmd_full, get_process_executor_mock};
use indexmap::IndexMap;
use shirabe::config::{Config, ConfigSourceInterface};
use shirabe::io::IOInterface;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::git::Git;
use shirabe::util::http_downloader::HttpDownloaderMockHandler;
use shirabe::util::process_executor::{MockExpectation, MockHandler, ProcessExecutor};
use shirabe_php_shim::{PhpMixed, RuntimeException};

// No-op ConfigSourceInterface, equivalent to PHPUnit's
// `getMockBuilder(Config::class)` auto-stubbing getConfigSource/getAuthConfigSource:
// the Bitbucket OAuth flow writes the token through these, but GitTest never asserts
// on them.
#[derive(Debug)]
struct NullConfigSource;

impl ConfigSourceInterface for NullConfigSource {
    fn add_repository(
        &mut self,
        _name: &str,
        _config: PhpMixed,
        _append: bool,
    ) -> anyhow::Result<()> {
        unreachable!()
    }
    fn insert_repository(
        &mut self,
        _name: &str,
        _config: PhpMixed,
        _reference_name: &str,
        _offset: i64,
    ) -> anyhow::Result<()> {
        unreachable!()
    }
    fn set_repository_url(&mut self, _name: &str, _url: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    fn remove_repository(&mut self, _name: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    fn add_config_setting(&mut self, _name: &str, _value: PhpMixed) -> anyhow::Result<()> {
        Ok(())
    }
    fn remove_config_setting(&mut self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn add_property(&mut self, _name: &str, _value: PhpMixed) -> anyhow::Result<()> {
        unreachable!()
    }
    fn remove_property(&mut self, _name: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    fn add_link(&mut self, _type: &str, _name: &str, _value: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    fn remove_link(&mut self, _type: &str, _name: &str) -> anyhow::Result<()> {
        unreachable!()
    }
    fn get_name(&self) -> String {
        "null".to_string()
    }
}

// PHP's `commandCallable` returns a bare string (`'git command'`); Rust's `run_command`
// flattens each callable to a `Vec<String>` and hands it to `execute_args`, which always
// builds a `PhpMixed::List`. So the single-token string command becomes a one-element list,
// and the corresponding process expectation is a one-element list as well.
fn build_git(
    io: IOStub,
    config: Config,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
) -> Git {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(io));
    let config = std::rc::Rc::new(std::cell::RefCell::new(config));
    let fs = std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None)));
    Git::new(io, config, process, fs)
}

fn mock_config(protocol: &str) -> Config {
    ConfigStubBuilder::new()
        .with(
            "github-domains",
            PhpMixed::List(vec![PhpMixed::String("github.com".to_string())]),
        )
        .with(
            "github-protocols",
            PhpMixed::List(vec![PhpMixed::String(protocol.to_string())]),
        )
        .build()
}

fn mock_sync_mirror_config() -> Config {
    ConfigStubBuilder::new()
        .with(
            "github-domains",
            PhpMixed::List(vec![PhpMixed::String("github.com".to_string())]),
        )
        .with(
            "gitlab-domains",
            PhpMixed::List(vec![PhpMixed::String("gitlab.com".to_string())]),
        )
        .with(
            "github-protocols",
            PhpMixed::List(vec![PhpMixed::String("https".to_string())]),
        )
        .build()
}

// publicGithubNoCredentialsProvider: ['ssh', 'git@github.com:acme/repo']
#[test]
fn test_run_command_public_git_hub_repository_not_initial_clone_ssh() {
    let expected_url = "git@github.com:acme/repo";
    let command_callable: Box<dyn Fn(&str) -> Vec<String>> = Box::new(move |url: &str| {
        assert_eq!(expected_url, url);
        vec!["git command".to_string()]
    });

    let config = mock_config("ssh");

    let (process, _guard) =
        get_process_executor_mock(vec![cmd(vec!["git command"])], true, MockHandler::default());

    let mut git = build_git(IOStub::new(), config, process);

    git.__run_command(
        vec![command_callable],
        "https://github.com/acme/repo",
        None,
        true,
        (),
    )
    .unwrap();
}

// publicGithubNoCredentialsProvider: ['https', 'https://github.com/acme/repo']
#[test]
fn test_run_command_public_git_hub_repository_not_initial_clone_https() {
    let expected_url = "https://github.com/acme/repo";
    let command_callable: Box<dyn Fn(&str) -> Vec<String>> = Box::new(move |url: &str| {
        assert_eq!(expected_url, url);
        vec!["git command".to_string()]
    });

    let config = mock_config("https");

    let (process, _guard) =
        get_process_executor_mock(vec![cmd(vec!["git command"])], true, MockHandler::default());

    let mut git = build_git(IOStub::new(), config, process);

    git.__run_command(
        vec![command_callable],
        "https://github.com/acme/repo",
        None,
        true,
        (),
    )
    .unwrap();
}

#[test]
fn test_run_command_private_git_hub_repository_not_initial_clone_not_interactive_without_authentication()
 {
    let command_callable: Box<dyn Fn(&str) -> Vec<String>> = Box::new(|url: &str| {
        assert_eq!("https://github.com/acme/repo", url);
        vec!["git command".to_string()]
    });

    let config = mock_config("https");

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd_full(vec!["git command"], 1, "", ""),
            cmd_full(vec!["git", "--version"], 0, "", ""),
        ],
        true,
        MockHandler::default(),
    );

    let mut git = build_git(IOStub::new(), config, process);

    let result = git.__run_command(
        vec![command_callable],
        "https://github.com/acme/repo",
        None,
        true,
        (),
    );

    let err = result.expect_err("expected a RuntimeException");
    assert!(err.downcast_ref::<RuntimeException>().is_some());
}

// privateGithubWithCredentialsProvider helper.
fn run_command_private_github_with_authentication(
    git_url: &str,
    protocol: &str,
    git_hub_token: &str,
    expected_url: &str,
    expected_failures_before_success: usize,
) {
    let expected_url_owned = expected_url.to_string();
    let command_callable: Box<dyn Fn(&str) -> Vec<String>> = Box::new(move |url: &str| {
        if url != expected_url_owned {
            return vec!["git command failing".to_string()];
        }
        vec!["git command ok".to_string()]
    });

    let config = mock_config(protocol);

    let mut expected_calls: Vec<MockExpectation> = Vec::new();
    for _ in 0..expected_failures_before_success {
        expected_calls.push(cmd_full(vec!["git command failing"], 1, "", ""));
    }
    expected_calls.push(cmd_full(vec!["git command ok"], 0, "", ""));

    let (process, _guard) = get_process_executor_mock(expected_calls, true, MockHandler::default());

    let mut auth: IndexMap<String, Option<String>> = IndexMap::new();
    auth.insert("username".to_string(), Some("token".to_string()));
    auth.insert("password".to_string(), Some(git_hub_token.to_string()));

    let io = IOStub::new()
        .with_is_interactive(false)
        .with_has_authentication(true)
        .with_get_authentication(auth);

    let mut git = build_git(io, config, process);

    git.__run_command(vec![command_callable], git_url, None, true, ())
        .unwrap();
}

#[test]
fn test_run_command_private_git_hub_repository_not_initial_clone_not_interactive_with_authentication_ssh()
 {
    run_command_private_github_with_authentication(
        "git@github.com:acme/repo.git",
        "ssh",
        "MY_GITHUB_TOKEN",
        "https://token:MY_GITHUB_TOKEN@github.com/acme/repo.git",
        1,
    );
}

#[test]
fn test_run_command_private_git_hub_repository_not_initial_clone_not_interactive_with_authentication_https()
 {
    run_command_private_github_with_authentication(
        "https://github.com/acme/repo",
        "https",
        "MY_GITHUB_TOKEN",
        "https://token:MY_GITHUB_TOKEN@github.com/acme/repo.git",
        2,
    );
}

// privateBitbucketWithCredentialsProvider helper.
fn run_command_private_bitbucket_with_authentication(
    git_url: &str,
    bitbucket_token: Option<&str>,
    expected_url: &str,
    expected_failures_before_success: usize,
    bitbucket_git_auth_calls: usize,
) {
    let expected_url_owned = expected_url.to_string();
    let command_callable: Box<dyn Fn(&str) -> Vec<String>> = Box::new(move |url: &str| {
        if url != expected_url_owned {
            return vec!["git command failing".to_string()];
        }
        vec!["git command ok".to_string()]
    });

    let config = ConfigStubBuilder::new()
        .with(
            "gitlab-domains",
            PhpMixed::List(vec![PhpMixed::String("gitlab.com".to_string())]),
        )
        .with(
            "github-domains",
            PhpMixed::List(vec![PhpMixed::String("github.com".to_string())]),
        )
        .build();

    let mut expected_calls: Vec<MockExpectation> = Vec::new();
    for _ in 0..expected_failures_before_success {
        expected_calls.push(cmd_full(vec!["git command failing"], 1, "", ""));
    }
    if bitbucket_git_auth_calls > 0 {
        for _ in 0..bitbucket_git_auth_calls {
            expected_calls.push(cmd_full(
                vec!["git", "config", "bitbucket.accesstoken"],
                1,
                "",
                "",
            ));
        }
    }
    expected_calls.push(cmd_full(vec!["git command ok"], 0, "", ""));

    let (process, _guard) = get_process_executor_mock(expected_calls, true, MockHandler::default());

    let mut io = IOStub::new().with_is_interactive(false);
    if let Some(token) = bitbucket_token {
        let mut auth: IndexMap<String, Option<String>> = IndexMap::new();
        auth.insert("username".to_string(), Some("token".to_string()));
        auth.insert("password".to_string(), Some(token.to_string()));
        io = io
            .with_has_authentication(true)
            .with_get_authentication(auth);
    }

    let mut git = build_git(io, config, process);

    git.__run_command(vec![command_callable], git_url, None, true, ())
        .unwrap();
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_ssh_token()
 {
    run_command_private_bitbucket_with_authentication(
        "git@bitbucket.org:acme/repo.git",
        Some("MY_BITBUCKET_TOKEN"),
        "https://token:MY_BITBUCKET_TOKEN@bitbucket.org/acme/repo.git",
        1,
        0,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_https_token()
 {
    run_command_private_bitbucket_with_authentication(
        "https://bitbucket.org/acme/repo",
        Some("MY_BITBUCKET_TOKEN"),
        "https://token:MY_BITBUCKET_TOKEN@bitbucket.org/acme/repo.git",
        1,
        0,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_https_git_token()
 {
    run_command_private_bitbucket_with_authentication(
        "https://bitbucket.org/acme/repo.git",
        Some("MY_BITBUCKET_TOKEN"),
        "https://token:MY_BITBUCKET_TOKEN@bitbucket.org/acme/repo.git",
        1,
        0,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_ssh_no_token()
 {
    run_command_private_bitbucket_with_authentication(
        "git@bitbucket.org:acme/repo.git",
        None,
        "git@bitbucket.org:acme/repo.git",
        0,
        0,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_https_no_token()
 {
    run_command_private_bitbucket_with_authentication(
        "https://bitbucket.org/acme/repo",
        None,
        "git@bitbucket.org:acme/repo.git",
        1,
        1,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_https_git_no_token()
 {
    run_command_private_bitbucket_with_authentication(
        "https://bitbucket.org/acme/repo.git",
        None,
        "git@bitbucket.org:acme/repo.git",
        1,
        1,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_atat_token()
 {
    run_command_private_bitbucket_with_authentication(
        "https://bitbucket.org/acme/repo.git",
        Some("ATAT_BITBUCKET_API_TOKEN"),
        "https://x-bitbucket-api-token-auth:ATAT_BITBUCKET_API_TOKEN@bitbucket.org/acme/repo.git",
        1,
        0,
    );
}

// privateBitbucketWithOauthProvider helper.
fn run_command_private_bitbucket_interactive_with_oauth(
    git_url: &str,
    expected_url: &str,
    initial_config: Option<(&str, &str)>,
) {
    let expected_url_owned = expected_url.to_string();
    let command_callable: Box<dyn Fn(&str) -> Vec<String>> = Box::new(move |url: &str| {
        if url != expected_url_owned {
            return vec!["git command failing".to_string()];
        }
        vec!["git command ok".to_string()]
    });

    let mut config = ConfigStubBuilder::new()
        .with(
            "gitlab-domains",
            PhpMixed::List(vec![PhpMixed::String("gitlab.com".to_string())]),
        )
        .with(
            "github-domains",
            PhpMixed::List(vec![PhpMixed::String("github.com".to_string())]),
        )
        .build();
    config.set_config_source(Box::new(NullConfigSource));
    config.set_auth_config_source(Box::new(NullConfigSource));

    let mut expected_calls: Vec<MockExpectation> = Vec::new();
    expected_calls.push(cmd_full(vec!["git command failing"], 1, "", ""));
    if initial_config.is_some() {
        expected_calls.push(cmd_full(vec!["git command failing"], 1, "", ""));
    } else {
        expected_calls.push(cmd_full(
            vec!["git", "config", "bitbucket.accesstoken"],
            1,
            "",
            "",
        ));
    }
    expected_calls.push(cmd_full(vec!["git command ok"], 0, "", ""));

    let (process, _guard) = get_process_executor_mock(expected_calls, true, MockHandler::default());

    let mut hidden_answers: IndexMap<String, String> = IndexMap::new();
    hidden_answers.insert(
        "Consumer Key (hidden): ".to_string(),
        "my-consumer-key".to_string(),
    );
    hidden_answers.insert(
        "Consumer Secret (hidden): ".to_string(),
        "my-consumer-secret".to_string(),
    );

    let mut io = IOStub::new()
        .with_is_interactive(true)
        .with_ask_confirmation(true)
        .with_ask_and_hide_answer_responses(hidden_answers);
    if let Some((username, password)) = initial_config {
        io = io.with_authentication("bitbucket.org", username, Some(password.to_string()));
    }

    let (downloader, _http_guard) = get_http_downloader_mock(
        vec![expect_full(
            "https://bitbucket.org/site/oauth2/access_token",
            None,
            200,
            r#"{"expires_in": 600, "access_token": "my-access-token"}"#,
            vec![],
        )],
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut git = build_git(io, config, process);
    git.set_http_downloader(downloader);

    git.__run_command(vec![command_callable], git_url, None, true, ())
        .unwrap();
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_interactive_with_oauth_ssh() {
    run_command_private_bitbucket_interactive_with_oauth(
        "git@bitbucket.org:acme/repo.git",
        "https://x-token-auth:my-access-token@bitbucket.org/acme/repo.git",
        None,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_interactive_with_oauth_https_git()
 {
    run_command_private_bitbucket_interactive_with_oauth(
        "https://bitbucket.org/acme/repo.git",
        "https://x-token-auth:my-access-token@bitbucket.org/acme/repo.git",
        None,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_interactive_with_oauth_https() {
    run_command_private_bitbucket_interactive_with_oauth(
        "https://bitbucket.org/acme/repo",
        "https://x-token-auth:my-access-token@bitbucket.org/acme/repo.git",
        None,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_interactive_with_oauth_preconfigured()
 {
    run_command_private_bitbucket_interactive_with_oauth(
        "git@bitbucket.org:acme/repo.git",
        "https://x-token-auth:my-access-token@bitbucket.org/acme/repo.git",
        Some(("someuseralsoswappedfortoken", "little green men")),
    );
}

#[test]
fn test_sync_mirror_sanitizes_url_after_initial_clone() {
    let non_existent_dir = format!(
        "{}/composer-test-nonexistent-{}",
        std::env::temp_dir().display(),
        rand_hex()
    );

    let config = mock_sync_mirror_config();

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd_full(
                vec![
                    "git",
                    "clone",
                    "--mirror",
                    "--",
                    "https://user:secret@example.com/repo.git",
                    &non_existent_dir,
                ],
                0,
                "",
                "",
            ),
            cmd_full(vec!["git", "remote", "-v"], 0, "", ""),
            cmd_full(
                vec![
                    "git",
                    "remote",
                    "set-url",
                    "origin",
                    "--",
                    "https://example.com/repo.git",
                ],
                0,
                "",
                "",
            ),
        ],
        true,
        MockHandler::default(),
    );

    let mut git = build_git(IOStub::new(), config, process);

    let result = git
        .sync_mirror(
            "https://user:secret@example.com/repo.git",
            &non_existent_dir,
        )
        .unwrap();

    assert!(result);
}

#[test]
fn test_sync_mirror_sanitizes_url_even_after_failed_update() {
    let dir = std::env::temp_dir().display().to_string();

    let config = mock_sync_mirror_config();

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd_full(vec!["git", "rev-parse", "--git-dir"], 0, ".\n", ""),
            cmd_full(vec!["git", "remote", "-v"], 0, "", ""),
            cmd_full(
                vec![
                    "git",
                    "remote",
                    "set-url",
                    "origin",
                    "--",
                    "https://user:secret@example.com/repo.git",
                ],
                0,
                "",
                "",
            ),
            cmd_full(
                vec!["git", "remote", "update", "--prune", "origin"],
                1,
                "",
                "",
            ),
            cmd_full(vec!["git", "--version"], 0, "", ""),
            cmd_full(vec!["git", "remote", "-v"], 0, "", ""),
            cmd_full(
                vec![
                    "git",
                    "remote",
                    "set-url",
                    "origin",
                    "--",
                    "https://example.com/repo.git",
                ],
                0,
                "",
                "",
            ),
        ],
        true,
        MockHandler::default(),
    );

    let mut git = build_git(IOStub::new(), config, process);

    let result = git
        .sync_mirror("https://user:secret@example.com/repo.git", &dir)
        .unwrap();

    assert!(!result);
}

fn rand_hex() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:016x}", nanos as u64)
}

//! ref: composer/tests/Composer/Test/Util/GitTest.php sha1:86ded931da91943a22fedb9168568216fe944720

use crate::config_stub::ConfigStubBuilder;
use crate::http_downloader_mock::{expect_full, get_http_downloader_mock};
use crate::io_stub::IOStub;
use crate::process_executor_mock::{cmd, cmd_full, get_process_executor_mock};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe::config::{Config, ConfigSourceInterface};
use shirabe::io::IOInterface;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::git::Git;
use shirabe::util::http_downloader::HttpDownloaderMockHandler;
use shirabe::util::process_executor::{CommandLine, MockExpectation, MockHandler, ProcessExecutor};
use shirabe_php_shim::{
    Catch as _, PhpMixed, RuntimeException, bconcat, display_to_bytes, path_from_bytes,
};

// No-op ConfigSourceInterface, equivalent to PHPUnit's
// `getMockBuilder(Config::class)` auto-stubbing getConfigSource/getAuthConfigSource:
// the Bitbucket OAuth flow writes the token through these, but GitTest never asserts
// on them.
#[derive(Debug)]
struct NullConfigSource;

impl ConfigSourceInterface for NullConfigSource {
    fn add_repository(
        &mut self,
        _name: &bstr::BStr,
        _config: PhpMixed,
        _append: bool,
    ) -> anyhow::Result<()> {
        unreachable!()
    }
    fn insert_repository(
        &mut self,
        _name: &bstr::BStr,
        _config: PhpMixed,
        _reference_name: &bstr::BStr,
        _offset: i64,
    ) -> anyhow::Result<()> {
        unreachable!()
    }
    fn set_repository_url(&mut self, _name: &bstr::BStr, _url: &bstr::BStr) -> anyhow::Result<()> {
        unreachable!()
    }
    fn remove_repository(&mut self, _name: &bstr::BStr) -> anyhow::Result<()> {
        unreachable!()
    }
    fn add_config_setting(&mut self, _name: &bstr::BStr, _value: PhpMixed) -> anyhow::Result<()> {
        Ok(())
    }
    fn remove_config_setting(&mut self, _name: &bstr::BStr) -> anyhow::Result<()> {
        Ok(())
    }
    fn add_property(&mut self, _name: &bstr::BStr, _value: PhpMixed) -> anyhow::Result<()> {
        unreachable!()
    }
    fn remove_property(&mut self, _name: &bstr::BStr) -> anyhow::Result<()> {
        unreachable!()
    }
    fn add_link(
        &mut self,
        _type: &bstr::BStr,
        _name: &bstr::BStr,
        _value: &bstr::BStr,
    ) -> anyhow::Result<()> {
        unreachable!()
    }
    fn remove_link(&mut self, _type: &bstr::BStr, _name: &bstr::BStr) -> anyhow::Result<()> {
        unreachable!()
    }
    fn get_name(&self) -> bstr::BString {
        "null".into()
    }
}

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

fn mock_config(protocol: &BStr) -> Config {
    ConfigStubBuilder::new()
        .with(
            "github-domains",
            PhpMixed::List(vec![PhpMixed::string(BString::from("github.com"))]),
        )
        .with(
            "github-protocols",
            PhpMixed::List(vec![PhpMixed::string(bconcat!(protocol))]),
        )
        .build()
}

fn mock_sync_mirror_config() -> Config {
    ConfigStubBuilder::new()
        .with(
            "github-domains",
            PhpMixed::List(vec![PhpMixed::string(BString::from("github.com"))]),
        )
        .with(
            "gitlab-domains",
            PhpMixed::List(vec![PhpMixed::string(BString::from("gitlab.com"))]),
        )
        .with(
            "github-protocols",
            PhpMixed::List(vec![PhpMixed::string(BString::from("https"))]),
        )
        .build()
}

// publicGithubNoCredentialsProvider: ['ssh', 'git@github.com:acme/repo']
#[test]
fn test_run_command_public_git_hub_repository_not_initial_clone_ssh() {
    let expected_url = "git@github.com:acme/repo";
    let command_callable: Box<dyn Fn(&bstr::BStr) -> CommandLine> =
        Box::new(move |url: &bstr::BStr| {
            assert_eq!(expected_url, url);
            CommandLine::Shell(BString::from("git command"))
        });

    let config = mock_config(BStr::new("ssh"));

    let (process, _guard) =
        get_process_executor_mock(vec![cmd("git command")], true, MockHandler::default());

    let mut git = build_git(IOStub::new(), config, process);

    git.__run_command(
        vec![command_callable],
        "https://github.com/acme/repo".into(),
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
    let command_callable: Box<dyn Fn(&bstr::BStr) -> CommandLine> =
        Box::new(move |url: &bstr::BStr| {
            assert_eq!(expected_url, url);
            CommandLine::Shell(BString::from("git command"))
        });

    let config = mock_config(BStr::new("https"));

    let (process, _guard) =
        get_process_executor_mock(vec![cmd("git command")], true, MockHandler::default());

    let mut git = build_git(IOStub::new(), config, process);

    git.__run_command(
        vec![command_callable],
        "https://github.com/acme/repo".into(),
        None,
        true,
        (),
    )
    .unwrap();
}

#[test]
fn test_run_command_private_git_hub_repository_not_initial_clone_not_interactive_without_authentication()
 {
    let command_callable: Box<dyn Fn(&bstr::BStr) -> CommandLine> = Box::new(|url: &bstr::BStr| {
        assert_eq!("https://github.com/acme/repo", url);
        CommandLine::Shell(BString::from("git command"))
    });

    let config = mock_config(BStr::new("https"));

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd_full("git command", 1, "", ""),
            cmd_full(vec!["git", "--version"], 0, "", ""),
        ],
        true,
        MockHandler::default(),
    );

    let mut git = build_git(IOStub::new(), config, process);

    let result = git.__run_command(
        vec![command_callable],
        "https://github.com/acme/repo".into(),
        None,
        true,
        (),
    );

    let err = result.expect_err("expected a RuntimeException");
    assert!(err.is_instanceof::<RuntimeException>());
}

// privateGithubWithCredentialsProvider helper.
fn run_command_private_github_with_authentication(
    git_url: &BStr,
    protocol: &BStr,
    git_hub_token: &BStr,
    expected_url: &BStr,
    expected_failures_before_success: usize,
) {
    let expected_url_owned = bconcat!(expected_url);
    let command_callable: Box<dyn Fn(&bstr::BStr) -> CommandLine> =
        Box::new(move |url: &bstr::BStr| {
            if url != expected_url_owned {
                return CommandLine::Shell(BString::from("git command failing"));
            }
            CommandLine::Shell(BString::from("git command ok"))
        });

    let config = mock_config(protocol);

    let mut expected_calls: Vec<MockExpectation> = Vec::new();
    for _ in 0..expected_failures_before_success {
        expected_calls.push(cmd_full("git command failing", 1, "", ""));
    }
    expected_calls.push(cmd_full("git command ok", 0, "", ""));

    let (process, _guard) = get_process_executor_mock(expected_calls, true, MockHandler::default());

    let mut auth: IndexMap<bstr::BString, Option<bstr::BString>> = IndexMap::new();
    auth.insert(
        BString::from("username"),
        Some(bstr::BString::from("token")),
    );
    auth.insert(
        BString::from("password"),
        Some(bstr::BString::from(git_hub_token)),
    );

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
        BStr::new("git@github.com:acme/repo.git"),
        BStr::new("ssh"),
        BStr::new("MY_GITHUB_TOKEN"),
        BStr::new("https://token:MY_GITHUB_TOKEN@github.com/acme/repo.git"),
        1,
    );
}

#[test]
fn test_run_command_private_git_hub_repository_not_initial_clone_not_interactive_with_authentication_https()
 {
    run_command_private_github_with_authentication(
        BStr::new("https://github.com/acme/repo"),
        BStr::new("https"),
        BStr::new("MY_GITHUB_TOKEN"),
        BStr::new("https://token:MY_GITHUB_TOKEN@github.com/acme/repo.git"),
        2,
    );
}

// privateBitbucketWithCredentialsProvider helper.
fn run_command_private_bitbucket_with_authentication(
    git_url: &BStr,
    bitbucket_token: Option<&BStr>,
    expected_url: &BStr,
    expected_failures_before_success: usize,
    bitbucket_git_auth_calls: usize,
) {
    let expected_url_owned = bconcat!(expected_url);
    let command_callable: Box<dyn Fn(&bstr::BStr) -> CommandLine> =
        Box::new(move |url: &bstr::BStr| {
            if url != expected_url_owned {
                return CommandLine::Shell(BString::from("git command failing"));
            }
            CommandLine::Shell(BString::from("git command ok"))
        });

    let config = ConfigStubBuilder::new()
        .with(
            "gitlab-domains",
            PhpMixed::List(vec![PhpMixed::string(BString::from("gitlab.com"))]),
        )
        .with(
            "github-domains",
            PhpMixed::List(vec![PhpMixed::string(BString::from("github.com"))]),
        )
        .build();

    let mut expected_calls: Vec<MockExpectation> = Vec::new();
    for _ in 0..expected_failures_before_success {
        expected_calls.push(cmd_full("git command failing", 1, "", ""));
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
    expected_calls.push(cmd_full("git command ok", 0, "", ""));

    let (process, _guard) = get_process_executor_mock(expected_calls, true, MockHandler::default());

    let mut io = IOStub::new().with_is_interactive(false);
    if let Some(token) = bitbucket_token {
        let mut auth: IndexMap<bstr::BString, Option<bstr::BString>> = IndexMap::new();
        auth.insert(
            BString::from("username"),
            Some(bstr::BString::from("token")),
        );
        auth.insert(BString::from("password"), Some(bstr::BString::from(token)));
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
        BStr::new("git@bitbucket.org:acme/repo.git"),
        Some(BStr::new("MY_BITBUCKET_TOKEN")),
        BStr::new("https://token:MY_BITBUCKET_TOKEN@bitbucket.org/acme/repo.git"),
        1,
        0,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_https_token()
 {
    run_command_private_bitbucket_with_authentication(
        BStr::new("https://bitbucket.org/acme/repo"),
        Some(BStr::new("MY_BITBUCKET_TOKEN")),
        BStr::new("https://token:MY_BITBUCKET_TOKEN@bitbucket.org/acme/repo.git"),
        1,
        0,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_https_git_token()
 {
    run_command_private_bitbucket_with_authentication(
        BStr::new("https://bitbucket.org/acme/repo.git"),
        Some(BStr::new("MY_BITBUCKET_TOKEN")),
        BStr::new("https://token:MY_BITBUCKET_TOKEN@bitbucket.org/acme/repo.git"),
        1,
        0,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_ssh_no_token()
 {
    run_command_private_bitbucket_with_authentication(
        BStr::new("git@bitbucket.org:acme/repo.git"),
        None,
        BStr::new("git@bitbucket.org:acme/repo.git"),
        0,
        0,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_https_no_token()
 {
    run_command_private_bitbucket_with_authentication(
        BStr::new("https://bitbucket.org/acme/repo"),
        None,
        BStr::new("git@bitbucket.org:acme/repo.git"),
        1,
        1,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_https_git_no_token()
 {
    run_command_private_bitbucket_with_authentication(
        BStr::new("https://bitbucket.org/acme/repo.git"),
        None,
        BStr::new("git@bitbucket.org:acme/repo.git"),
        1,
        1,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_not_interactive_with_authentication_atat_token()
 {
    run_command_private_bitbucket_with_authentication(
        BStr::new("https://bitbucket.org/acme/repo.git"),
        Some(BStr::new("ATAT_BITBUCKET_API_TOKEN")),
        BStr::new(
            "https://x-bitbucket-api-token-auth:ATAT_BITBUCKET_API_TOKEN@bitbucket.org/acme/repo.git",
        ),
        1,
        0,
    );
}

// privateBitbucketWithOauthProvider helper.
fn run_command_private_bitbucket_interactive_with_oauth(
    git_url: &BStr,
    expected_url: &BStr,
    initial_config: Option<(&BStr, &BStr)>,
) {
    let expected_url_owned = bconcat!(expected_url);
    let command_callable: Box<dyn Fn(&bstr::BStr) -> CommandLine> =
        Box::new(move |url: &bstr::BStr| {
            if url != expected_url_owned {
                return CommandLine::Shell(BString::from("git command failing"));
            }
            CommandLine::Shell(BString::from("git command ok"))
        });

    let mut config = ConfigStubBuilder::new()
        .with(
            "gitlab-domains",
            PhpMixed::List(vec![PhpMixed::string(BString::from("gitlab.com"))]),
        )
        .with(
            "github-domains",
            PhpMixed::List(vec![PhpMixed::string(BString::from("github.com"))]),
        )
        .build();
    config.set_config_source(Box::new(NullConfigSource));
    config.set_auth_config_source(Box::new(NullConfigSource));

    let mut expected_calls: Vec<MockExpectation> = Vec::new();
    expected_calls.push(cmd_full("git command failing", 1, "", ""));
    if initial_config.is_some() {
        expected_calls.push(cmd_full("git command failing", 1, "", ""));
    } else {
        expected_calls.push(cmd_full(
            vec!["git", "config", "bitbucket.accesstoken"],
            1,
            "",
            "",
        ));
    }
    expected_calls.push(cmd_full("git command ok", 0, "", ""));

    let (process, _guard) = get_process_executor_mock(expected_calls, true, MockHandler::default());

    let mut hidden_answers: IndexMap<BString, BString> = IndexMap::new();
    hidden_answers.insert(
        BString::from("Consumer Key (hidden): "),
        BString::from("my-consumer-key"),
    );
    hidden_answers.insert(
        BString::from("Consumer Secret (hidden): "),
        BString::from("my-consumer-secret"),
    );

    let mut io = IOStub::new()
        .with_is_interactive(true)
        .with_ask_confirmation(true)
        .with_ask_and_hide_answer_responses(hidden_answers);
    if let Some((username, password)) = initial_config {
        io = io.with_authentication("bitbucket.org", username, Some(bconcat!(password)));
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
        BStr::new("git@bitbucket.org:acme/repo.git"),
        BStr::new("https://x-token-auth:my-access-token@bitbucket.org/acme/repo.git"),
        None,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_interactive_with_oauth_https_git()
 {
    run_command_private_bitbucket_interactive_with_oauth(
        BStr::new("https://bitbucket.org/acme/repo.git"),
        BStr::new("https://x-token-auth:my-access-token@bitbucket.org/acme/repo.git"),
        None,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_interactive_with_oauth_https() {
    run_command_private_bitbucket_interactive_with_oauth(
        BStr::new("https://bitbucket.org/acme/repo"),
        BStr::new("https://x-token-auth:my-access-token@bitbucket.org/acme/repo.git"),
        None,
    );
}

#[test]
fn test_run_command_private_bitbucket_repository_not_initial_clone_interactive_with_oauth_preconfigured()
 {
    run_command_private_bitbucket_interactive_with_oauth(
        BStr::new("git@bitbucket.org:acme/repo.git"),
        BStr::new("https://x-token-auth:my-access-token@bitbucket.org/acme/repo.git"),
        Some((
            BStr::new("someuseralsoswappedfortoken"),
            BStr::new("little green men"),
        )),
    );
}

#[test]
fn test_sync_mirror_sanitizes_url_after_initial_clone() {
    let non_existent_dir = bconcat!(std::env::temp_dir().join(path_from_bytes(&bconcat!(
        "composer-test-nonexistent-",
        rand_hex()
    ))));

    let config = mock_sync_mirror_config();

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd_full(
                vec![
                    BStr::new("git"),
                    BStr::new("clone"),
                    BStr::new("--mirror"),
                    BStr::new("--"),
                    BStr::new("https://user:secret@example.com/repo.git"),
                    non_existent_dir.as_bstr(),
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
            "https://user:secret@example.com/repo.git".into(),
            path_from_bytes(&non_existent_dir),
        )
        .unwrap();

    assert!(result);
}

#[test]
fn test_sync_mirror_sanitizes_url_even_after_failed_update() {
    let dir = bconcat!(std::env::temp_dir());

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
        .sync_mirror(
            "https://user:secret@example.com/repo.git".into(),
            path_from_bytes(&dir),
        )
        .unwrap();

    assert!(!result);
}

fn rand_hex() -> BString {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    display_to_bytes(format_args!("{:016x}", nanos as u64))
}

//! ref: composer/tests/Composer/Test/Util/ForgejoUrlTest.php

use shirabe::util::forgejo_url::ForgejoUrl;

#[test]
#[ignore = "Preg::match panics: ForgejoUrl::URL_REGEX is an undelimited pattern"]
fn test_create() {
    for repo_url in create_provider() {
        let forgejo_url = ForgejoUrl::try_from(Some(repo_url));

        assert!(forgejo_url.is_some());
        let forgejo_url = forgejo_url.unwrap();
        assert_eq!("codeberg.org", forgejo_url.origin_url);
        assert_eq!("acme", forgejo_url.owner);
        assert_eq!("repo", forgejo_url.repository);
        assert_eq!(
            "https://codeberg.org/api/v1/repos/acme/repo",
            forgejo_url.api_url
        );
    }
}

fn create_provider() -> Vec<&'static str> {
    vec![
        "git@codeberg.org:acme/repo.git",
        "https://codeberg.org/acme/repo",
        "https://codeberg.org/acme/repo.git",
    ]
}

#[test]
#[ignore = "Preg::match panics: ForgejoUrl::URL_REGEX is an undelimited pattern"]
fn test_create_invalid() {
    assert!(ForgejoUrl::create("https://example.org").is_err());
}

#[test]
#[ignore = "Preg::match panics: ForgejoUrl::URL_REGEX is an undelimited pattern"]
fn test_generate_ssh_url() {
    let forgejo_url = ForgejoUrl::create("git@codeberg.org:acme/repo.git").unwrap();

    assert_eq!(
        "git@codeberg.org:acme/repo.git",
        forgejo_url.generate_ssh_url()
    );
}

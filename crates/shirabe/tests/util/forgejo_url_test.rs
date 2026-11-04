//! ref: composer/tests/Composer/Test/Util/ForgejoUrlTest.php sha1:bf985806e781c179d6d422aee9d15fc1ad8a93a7

use bstr::BStr;
use shirabe::util::forgejo_url::ForgejoUrl;

#[test]
fn test_create() {
    for repo_url in create_provider() {
        let forgejo_url = ForgejoUrl::try_from(Some(bstr::BStr::new(repo_url)));

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

fn create_provider() -> Vec<&'static BStr> {
    vec![
        BStr::new("git@codeberg.org:acme/repo.git"),
        BStr::new("https://codeberg.org/acme/repo"),
        BStr::new("https://codeberg.org/acme/repo.git"),
    ]
}

#[test]
fn test_create_invalid() {
    assert!(ForgejoUrl::create(bstr::BStr::new("https://example.org")).is_err());
}

#[test]
fn test_generate_ssh_url() {
    let forgejo_url =
        ForgejoUrl::create(bstr::BStr::new("git@codeberg.org:acme/repo.git")).unwrap();

    assert_eq!(
        "git@codeberg.org:acme/repo.git",
        forgejo_url.generate_ssh_url()
    );
}

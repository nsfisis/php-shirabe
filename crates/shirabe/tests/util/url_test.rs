//! ref: composer/tests/Composer/Test/Util/UrlTest.php

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::util::url::Url;
use shirabe_php_shim::PhpMixed;

fn conf(entries: &[(&str, &[&str])]) -> IndexMap<String, PhpMixed> {
    entries
        .iter()
        .map(|(k, vals)| {
            (
                k.to_string(),
                PhpMixed::List(
                    vals.iter()
                        .map(|v| PhpMixed::String(v.to_string()))
                        .collect(),
                ),
            )
        })
        .collect()
}

#[test]
#[ignore]
fn test_update_dist_reference() {
    for (url, expected_url, c, r#ref) in dist_refs_provider() {
        let mut config = Config::new(true, None);
        let mut merged: IndexMap<String, PhpMixed> = IndexMap::new();
        merged.insert("config".to_string(), PhpMixed::Array(c));
        config.merge(&merged, Config::SOURCE_UNKNOWN);

        assert_eq!(
            expected_url,
            Url::update_dist_reference(&config, url.to_string(), r#ref)
        );
    }
}

fn dist_refs_provider() -> Vec<(
    &'static str,
    &'static str,
    IndexMap<String, PhpMixed>,
    &'static str,
)> {
    vec![
        // github
        (
            "https://github.com/foo/bar/zipball/abcd",
            "https://api.github.com/repos/foo/bar/zipball/newref",
            conf(&[]),
            "newref",
        ),
        (
            "https://www.github.com/foo/bar/zipball/abcd",
            "https://api.github.com/repos/foo/bar/zipball/newref",
            conf(&[]),
            "newref",
        ),
        (
            "https://github.com/foo/bar/archive/abcd.zip",
            "https://api.github.com/repos/foo/bar/zipball/newref",
            conf(&[]),
            "newref",
        ),
        (
            "https://github.com/foo/bar/archive/abcd.tar.gz",
            "https://api.github.com/repos/foo/bar/tarball/newref",
            conf(&[]),
            "newref",
        ),
        (
            "https://api.github.com/repos/foo/bar/tarball",
            "https://api.github.com/repos/foo/bar/tarball/newref",
            conf(&[]),
            "newref",
        ),
        (
            "https://api.github.com/repos/foo/bar/tarball/abcd",
            "https://api.github.com/repos/foo/bar/tarball/newref",
            conf(&[]),
            "newref",
        ),
        // github enterprise
        (
            "https://mygithub.com/api/v3/repos/foo/bar/tarball/abcd",
            "https://mygithub.com/api/v3/repos/foo/bar/tarball/newref",
            conf(&[("github-domains", &["mygithub.com"])]),
            "newref",
        ),
        // bitbucket
        (
            "https://bitbucket.org/foo/bar/get/abcd.zip",
            "https://bitbucket.org/foo/bar/get/newref.zip",
            conf(&[]),
            "newref",
        ),
        (
            "https://www.bitbucket.org/foo/bar/get/abcd.tar.bz2",
            "https://bitbucket.org/foo/bar/get/newref.tar.bz2",
            conf(&[]),
            "newref",
        ),
        // gitlab
        (
            "https://gitlab.com/api/v4/projects/foo%2Fbar/repository/archive.zip?sha=abcd",
            "https://gitlab.com/api/v4/projects/foo%2Fbar/repository/archive.zip?sha=newref",
            conf(&[]),
            "newref",
        ),
        (
            "https://www.gitlab.com/api/v4/projects/foo%2Fbar/repository/archive.zip?sha=abcd",
            "https://gitlab.com/api/v4/projects/foo%2Fbar/repository/archive.zip?sha=newref",
            conf(&[]),
            "newref",
        ),
        (
            "https://gitlab.com/api/v3/projects/foo%2Fbar/repository/archive.tar.gz?sha=abcd",
            "https://gitlab.com/api/v4/projects/foo%2Fbar/repository/archive.tar.gz?sha=newref",
            conf(&[]),
            "newref",
        ),
        // gitlab enterprise
        (
            "https://mygitlab.com/api/v4/projects/foo%2Fbar/repository/archive.tar.gz?sha=abcd",
            "https://mygitlab.com/api/v4/projects/foo%2Fbar/repository/archive.tar.gz?sha=newref",
            conf(&[("gitlab-domains", &["mygitlab.com"])]),
            "newref",
        ),
        (
            "https://mygitlab.com/api/v3/projects/foo%2Fbar/repository/archive.tar.bz2?sha=abcd",
            "https://mygitlab.com/api/v3/projects/foo%2Fbar/repository/archive.tar.bz2?sha=newref",
            conf(&[("gitlab-domains", &["mygitlab.com"])]),
            "newref",
        ),
        (
            "https://mygitlab.com/api/v3/projects/foo%2Fbar/repository/archive.tar.bz2?sha=abcd",
            "https://mygitlab.com/api/v3/projects/foo%2Fbar/repository/archive.tar.bz2?sha=65",
            conf(&[("gitlab-domains", &["mygitlab.com"])]),
            "65",
        ),
    ]
}

#[test]
fn test_sanitize() {
    for (expected, url) in sanitize_provider() {
        assert_eq!(expected, Url::sanitize(url.to_string()));
    }
}

fn sanitize_provider() -> Vec<(&'static str, &'static str)> {
    vec![
        // with scheme
        (
            "https://foo:***@example.org/",
            "https://foo:bar@example.org/",
        ),
        ("https://foo@example.org/", "https://foo@example.org/"),
        ("https://example.org/", "https://example.org/"),
        (
            "http://***:***@example.org",
            "http://10a8f08e8d7b7b9:foo@example.org",
        ),
        (
            "https://foo:***@example.org:123/",
            "https://foo:bar@example.org:123/",
        ),
        (
            "https://example.org/foo/bar?access_token=***",
            "https://example.org/foo/bar?access_token=abcdef",
        ),
        (
            "https://example.org/foo/bar?foo=bar&access_token=***",
            "https://example.org/foo/bar?foo=bar&access_token=abcdef",
        ),
        (
            "https://***:***@github.com/acme/repo",
            "https://ghp_1234567890abcdefghijklmnopqrstuvwxyzAB:x-oauth-basic@github.com/acme/repo",
        ),
        (
            "https://***:***@github.com/acme/repo",
            "https://github_pat_1234567890abcdefghijkl_1234567890abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVW:x-oauth-basic@github.com/acme/repo",
        ),
        // without scheme
        ("foo:***@example.org/", "foo:bar@example.org/"),
        ("foo@example.org/", "foo@example.org/"),
        ("example.org/", "example.org/"),
        ("***:***@example.org", "10a8f08e8d7b7b9:foo@example.org"),
        ("foo:***@example.org:123/", "foo:bar@example.org:123/"),
        (
            "example.org/foo/bar?access_token=***",
            "example.org/foo/bar?access_token=abcdef",
        ),
        (
            "example.org/foo/bar?foo=bar&access_token=***",
            "example.org/foo/bar?foo=bar&access_token=abcdef",
        ),
    ]
}

//! ref: composer/tests/Composer/Test/Util/UrlTest.php sha1:800df887918797f0a3d414af0a5c000bcb90887a

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::util::Url;
use shirabe_php_shim::{PhpMixed, bconcat};

fn conf(entries: &[(&BStr, &[&BStr])]) -> IndexMap<BString, PhpMixed> {
    entries
        .iter()
        .map(|(k, vals)| {
            (
                bconcat!(k),
                PhpMixed::List(vals.iter().map(|v| PhpMixed::string(bconcat!(v))).collect()),
            )
        })
        .collect()
}

#[test]
fn test_update_dist_reference() {
    for (url, expected_url, c, r#ref) in dist_refs_provider() {
        let mut config = Config::new(true, None);
        let mut merged: IndexMap<BString, PhpMixed> = IndexMap::new();
        merged.insert("config".into(), PhpMixed::Array(c));
        config.merge(&merged, Config::SOURCE_UNKNOWN);

        assert_eq!(
            expected_url,
            Url::update_dist_reference(&config, url, r#ref).unwrap()
        );
    }
}

fn dist_refs_provider() -> Vec<(
    &'static BStr,
    &'static BStr,
    IndexMap<BString, PhpMixed>,
    &'static BStr,
)> {
    vec![
        // github
        (
            BStr::new("https://github.com/foo/bar/zipball/abcd"),
            BStr::new("https://api.github.com/repos/foo/bar/zipball/newref"),
            conf(&[]),
            BStr::new("newref"),
        ),
        (
            BStr::new("https://www.github.com/foo/bar/zipball/abcd"),
            BStr::new("https://api.github.com/repos/foo/bar/zipball/newref"),
            conf(&[]),
            BStr::new("newref"),
        ),
        (
            BStr::new("https://github.com/foo/bar/archive/abcd.zip"),
            BStr::new("https://api.github.com/repos/foo/bar/zipball/newref"),
            conf(&[]),
            BStr::new("newref"),
        ),
        (
            BStr::new("https://github.com/foo/bar/archive/abcd.tar.gz"),
            BStr::new("https://api.github.com/repos/foo/bar/tarball/newref"),
            conf(&[]),
            BStr::new("newref"),
        ),
        (
            BStr::new("https://api.github.com/repos/foo/bar/tarball"),
            BStr::new("https://api.github.com/repos/foo/bar/tarball/newref"),
            conf(&[]),
            BStr::new("newref"),
        ),
        (
            BStr::new("https://api.github.com/repos/foo/bar/tarball/abcd"),
            BStr::new("https://api.github.com/repos/foo/bar/tarball/newref"),
            conf(&[]),
            BStr::new("newref"),
        ),
        // github enterprise
        (
            BStr::new("https://mygithub.com/api/v3/repos/foo/bar/tarball/abcd"),
            BStr::new("https://mygithub.com/api/v3/repos/foo/bar/tarball/newref"),
            conf(&[(BStr::new("github-domains"), &[BStr::new("mygithub.com")])]),
            BStr::new("newref"),
        ),
        // bitbucket
        (
            BStr::new("https://bitbucket.org/foo/bar/get/abcd.zip"),
            BStr::new("https://bitbucket.org/foo/bar/get/newref.zip"),
            conf(&[]),
            BStr::new("newref"),
        ),
        (
            BStr::new("https://www.bitbucket.org/foo/bar/get/abcd.tar.bz2"),
            BStr::new("https://bitbucket.org/foo/bar/get/newref.tar.bz2"),
            conf(&[]),
            BStr::new("newref"),
        ),
        // gitlab
        (
            BStr::new(
                "https://gitlab.com/api/v4/projects/foo%2Fbar/repository/archive.zip?sha=abcd",
            ),
            BStr::new(
                "https://gitlab.com/api/v4/projects/foo%2Fbar/repository/archive.zip?sha=newref",
            ),
            conf(&[]),
            BStr::new("newref"),
        ),
        (
            BStr::new(
                "https://www.gitlab.com/api/v4/projects/foo%2Fbar/repository/archive.zip?sha=abcd",
            ),
            BStr::new(
                "https://gitlab.com/api/v4/projects/foo%2Fbar/repository/archive.zip?sha=newref",
            ),
            conf(&[]),
            BStr::new("newref"),
        ),
        (
            BStr::new(
                "https://gitlab.com/api/v3/projects/foo%2Fbar/repository/archive.tar.gz?sha=abcd",
            ),
            BStr::new(
                "https://gitlab.com/api/v4/projects/foo%2Fbar/repository/archive.tar.gz?sha=newref",
            ),
            conf(&[]),
            BStr::new("newref"),
        ),
        // gitlab enterprise
        (
            BStr::new(
                "https://mygitlab.com/api/v4/projects/foo%2Fbar/repository/archive.tar.gz?sha=abcd",
            ),
            BStr::new(
                "https://mygitlab.com/api/v4/projects/foo%2Fbar/repository/archive.tar.gz?sha=newref",
            ),
            conf(&[(BStr::new("gitlab-domains"), &[BStr::new("mygitlab.com")])]),
            BStr::new("newref"),
        ),
        (
            BStr::new(
                "https://mygitlab.com/api/v3/projects/foo%2Fbar/repository/archive.tar.bz2?sha=abcd",
            ),
            BStr::new(
                "https://mygitlab.com/api/v3/projects/foo%2Fbar/repository/archive.tar.bz2?sha=newref",
            ),
            conf(&[(BStr::new("gitlab-domains"), &[BStr::new("mygitlab.com")])]),
            BStr::new("newref"),
        ),
        (
            BStr::new(
                "https://mygitlab.com/api/v3/projects/foo%2Fbar/repository/archive.tar.bz2?sha=abcd",
            ),
            BStr::new(
                "https://mygitlab.com/api/v3/projects/foo%2Fbar/repository/archive.tar.bz2?sha=65",
            ),
            conf(&[(BStr::new("gitlab-domains"), &[BStr::new("mygitlab.com")])]),
            BStr::new("65"),
        ),
    ]
}

#[test]
fn test_sanitize() {
    for (expected, url) in sanitize_provider() {
        assert_eq!(expected, Url::sanitize(url));
    }
}

fn sanitize_provider() -> Vec<(&'static BStr, &'static BStr)> {
    vec![
        // with scheme
        (
            BStr::new("https://foo:***@example.org/"),
            BStr::new("https://foo:bar@example.org/"),
        ),
        (
            BStr::new("https://foo@example.org/"),
            BStr::new("https://foo@example.org/"),
        ),
        (
            BStr::new("https://example.org/"),
            BStr::new("https://example.org/"),
        ),
        (
            BStr::new("http://***:***@example.org"),
            BStr::new("http://10a8f08e8d7b7b9:foo@example.org"),
        ),
        (
            BStr::new("https://foo:***@example.org:123/"),
            BStr::new("https://foo:bar@example.org:123/"),
        ),
        (
            BStr::new("https://example.org/foo/bar?access_token=***"),
            BStr::new("https://example.org/foo/bar?access_token=abcdef"),
        ),
        (
            BStr::new("https://example.org/foo/bar?foo=bar&access_token=***"),
            BStr::new("https://example.org/foo/bar?foo=bar&access_token=abcdef"),
        ),
        (
            BStr::new("https://***:***@github.com/acme/repo"),
            BStr::new(
                "https://ghp_1234567890abcdefghijklmnopqrstuvwxyzAB:x-oauth-basic@github.com/acme/repo",
            ),
        ),
        (
            BStr::new("https://***:***@github.com/acme/repo"),
            BStr::new(
                "https://github_pat_1234567890abcdefghijkl_1234567890abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVW:x-oauth-basic@github.com/acme/repo",
            ),
        ),
        // without scheme
        (
            BStr::new("foo:***@example.org/"),
            BStr::new("foo:bar@example.org/"),
        ),
        (BStr::new("foo@example.org/"), BStr::new("foo@example.org/")),
        (BStr::new("example.org/"), BStr::new("example.org/")),
        (
            BStr::new("***:***@example.org"),
            BStr::new("10a8f08e8d7b7b9:foo@example.org"),
        ),
        (
            BStr::new("foo:***@example.org:123/"),
            BStr::new("foo:bar@example.org:123/"),
        ),
        (
            BStr::new("example.org/foo/bar?access_token=***"),
            BStr::new("example.org/foo/bar?access_token=abcdef"),
        ),
        (
            BStr::new("example.org/foo/bar?foo=bar&access_token=***"),
            BStr::new("example.org/foo/bar?foo=bar&access_token=abcdef"),
        ),
    ]
}

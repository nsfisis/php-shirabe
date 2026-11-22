//! ref: composer/tests/Composer/Test/IO/BaseIOTest.php sha1:fd31390a3495f20359947a214ca5e552e6f7a07f

use bstr::{BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::{BufferIO, IOInterface};
use shirabe_php_shim::{Catch as _, PhpMixed, UnexpectedValueException, error_message};
use shirabe_symfony_console::output::VERBOSITY_NORMAL;

fn github_oauth_config(token: &BStr) -> IndexMap<BString, PhpMixed> {
    let mut github_oauth = IndexMap::new();
    github_oauth.insert(
        BString::from("github.com"),
        PhpMixed::string(token.to_owned()),
    );
    let mut config = IndexMap::new();
    config.insert(BString::from("github-oauth"), PhpMixed::Array(github_oauth));
    let mut outer = IndexMap::new();
    outer.insert(BString::from("config"), PhpMixed::Array(config));
    outer
}

#[test]
fn test_load_configuration_accepts_valid_github_token() {
    for (name, token) in provide_valid_github_tokens() {
        let io = BufferIO::new(BString::default(), VERBOSITY_NORMAL, None).unwrap();
        let config = Config::new(false, None);
        config.merge(&github_oauth_config(token), Config::SOURCE_UNKNOWN);

        io.load_configuration(&config)
            .unwrap_or_else(|e| panic!("data set {name:?}: {e}"));

        let auth = io.get_authentication(BStr::new("github.com"));
        assert_eq!(
            Some(&Some(token.to_owned())),
            auth.get(BStr::new("username")),
            "data set {name:?}"
        );
        assert_eq!(
            Some(&Some(BString::from("x-oauth-basic"))),
            auth.get(BStr::new("password")),
            "data set {name:?}"
        );
    }
}

fn provide_valid_github_tokens() -> Vec<(&'static BStr, &'static BStr)> {
    vec![
        (
            BStr::new("legacy 40-hex PAT"),
            BStr::new("8a7f2c1bdc4e9f06a3b7c2e9d4f1a8b6c5d7e0f2"),
        ),
        (
            BStr::new("ghp_ flat token"),
            BStr::new("ghp_n3K9wQ2eL5bV8mY1pX4cZ7aR0fT6sH3uJ8oI"),
        ),
        (
            BStr::new("gho_ flat token"),
            BStr::new("gho_M2pQ7vR4xL9eK6bN1cT8aZ0sJ3wY5fH7uG2d"),
        ),
        (
            BStr::new("ghu_ flat token"),
            BStr::new("ghu_R5tY8wA1xC4eK7bN0pV3mL6sH9uJ2gD5fQ8z"),
        ),
        (
            BStr::new("ghs_ flat token"),
            BStr::new("ghs_K7bN3pV5mL8eR2tY9wA1xC4sH6uJ0gD3fQ8z"),
        ),
        (
            BStr::new("ghr_ flat token"),
            BStr::new("ghr_X9aZ2sJ5wY8fH1uG4dR7bN0pV3mL6eK2tQ5c"),
        ),
        // shivammathur/setup-php style: ghs_<id>_<base64url>.<base64url>.<base64url>
        // base64url alphabet includes '-' which the old regex rejected
        (
            BStr::new("ghs_ structured installation token (jwt body)"),
            BStr::new(BStr::new(
                "ghs_1234567890_eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJjb21wb3NlciJ9.aB-cDef_GHIjkl-mnoPQR0123456",
            )),
        ),
        (
            BStr::new("github_pat_ fine-grained PAT"),
            BStr::new(BStr::new(
                "github_pat_11ABCDEFG0aB1cD2eF3gH4_n3K9wQ2eL5bV8mY1pX4cZ7aR0fT6sH3uJ8oI2pQ7vR4xL9eK6bN",
            )),
        ),
    ]
}

#[test]
fn test_load_configuration_rejects_token_with_url_breaking_characters() {
    for (name, token, offending) in provide_url_breaking_github_tokens() {
        let io = BufferIO::new(BString::default(), VERBOSITY_NORMAL, None).unwrap();
        let config = Config::new(false, None);
        config.merge(&github_oauth_config(token), Config::SOURCE_UNKNOWN);

        match io.load_configuration(&config) {
            Ok(()) => panic!(
                "data set {name:?}: Expected loadConfiguration to reject token containing {offending:?}"
            ),
            Err(e) if e.catch::<UnexpectedValueException>().is_some() => {
                // Defect #1: the rejected token must not be echoed back into the
                // exception message — Symfony Console renders it to stderr and CI
                // log shippers / GitHub Actions secret masking do not reliably
                // strip it from the framed error block.
                assert!(
                    !error_message(&e).contains_str(token),
                    "data set {name:?}: Exception message must not leak the rejected token value."
                );
            }
            Err(e) => panic!("data set {name:?}: {e}"),
        }
    }
}

fn provide_url_breaking_github_tokens() -> Vec<(&'static BStr, &'static BStr, &'static BStr)> {
    vec![
        (
            BStr::new("contains @ (userinfo separator)"),
            BStr::new("ghp_AAAA@evil.example.com"),
            BStr::new("@"),
        ),
        (
            BStr::new("contains : (basic-auth user:pass split)"),
            BStr::new("ghp_AAAA:extra"),
            BStr::new(":"),
        ),
        (
            BStr::new("contains / (path separator)"),
            BStr::new("ghp_AAA/BBB"),
            BStr::new("/"),
        ),
        (
            BStr::new("contains backslash"),
            BStr::new("ghp_AAA\\BBB"),
            BStr::new("\\"),
        ),
        (
            BStr::new("contains ? (query separator)"),
            BStr::new("ghp_AAA?x=1"),
            BStr::new("?"),
        ),
        (
            BStr::new("contains # (fragment)"),
            BStr::new("ghp_AAA#frag"),
            BStr::new("#"),
        ),
        (
            BStr::new("contains space"),
            BStr::new("ghp_AAA BBB"),
            BStr::new("space"),
        ),
        (
            BStr::new("contains tab"),
            BStr::new("ghp_AAA\tBBB"),
            BStr::new("tab"),
        ),
        (
            BStr::new("contains CR"),
            BStr::new("ghp_AAA\rBBB"),
            BStr::new("CR"),
        ),
        (
            BStr::new("contains LF (header injection)"),
            BStr::new("ghp_AAA\nX-Evil: 1"),
            BStr::new("LF"),
        ),
    ]
}

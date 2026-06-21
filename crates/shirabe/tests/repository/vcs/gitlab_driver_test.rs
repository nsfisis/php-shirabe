//! ref: composer/tests/Composer/Test/Repository/Vcs/GitLabDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::GitLabDriver;
use shirabe_php_shim::{PhpMixed, extension_loaded};

// Mirrors GitLabDriverTest::setUp's `gitlab-domains` configuration.
fn make_config() -> Config {
    let mut config = Config::new(true, None);
    let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<String, PhpMixed> = IndexMap::new();
    config_section.insert(
        "gitlab-domains".to_string(),
        PhpMixed::List(vec![
            PhpMixed::String("mycompany.com/gitlab".to_string()),
            PhpMixed::String("gitlab.mycompany.com".to_string()),
            PhpMixed::String("othercompany.com/nested/gitlab".to_string()),
            PhpMixed::String("gitlab.com".to_string()),
            PhpMixed::String("gitlab.mycompany.local".to_string()),
        ]),
    );
    top.insert("config".to_string(), PhpMixed::Array(config_section));
    config.merge(&top, Config::SOURCE_UNKNOWN);
    config
}

#[test]
#[ignore = "HttpDownloaderMock (getHttpDownloaderMock) and the IOInterface MockObject are not ported"]
fn test_initialize() {
    todo!()
}

#[test]
#[ignore = "HttpDownloaderMock (getHttpDownloaderMock) and the IOInterface MockObject are not ported"]
fn test_initialize_public_project() {
    todo!()
}

#[test]
#[ignore = "HttpDownloaderMock (getHttpDownloaderMock) and the IOInterface MockObject are not ported"]
fn test_initialize_public_project_as_anonymous() {
    todo!()
}

#[test]
#[ignore = "HttpDownloaderMock (getHttpDownloaderMock) and the IOInterface MockObject are not ported"]
fn test_initialize_with_port_number() {
    todo!()
}

#[test]
#[ignore = "depends on testInitialize, which needs HttpDownloaderMock + setAttribute (\\ReflectionProperty) not ported"]
fn test_invalid_support_data() {
    todo!()
}

#[test]
#[ignore = "depends on testInitialize, which needs HttpDownloaderMock + the IOInterface MockObject not ported"]
fn test_get_dist() {
    todo!()
}

#[test]
#[ignore = "depends on testInitialize, which needs HttpDownloaderMock + the IOInterface MockObject not ported"]
fn test_get_source() {
    todo!()
}

#[test]
#[ignore = "depends on testInitializePublicProject, which needs HttpDownloaderMock + the IOInterface MockObject not ported"]
fn test_get_source_given_public_project() {
    todo!()
}

#[test]
#[ignore = "depends on testInitialize, which needs HttpDownloaderMock + the IOInterface MockObject not ported"]
fn test_get_tags() {
    todo!()
}

#[test]
#[ignore = "depends on testInitialize, which needs HttpDownloaderMock + the IOInterface MockObject not ported"]
fn test_get_paginated_refs() {
    todo!()
}

#[test]
#[ignore = "depends on testInitialize, which needs HttpDownloaderMock + the IOInterface MockObject not ported"]
fn test_get_branches() {
    todo!()
}

#[test]
#[ignore]
fn test_supports() {
    for (url, expected) in data_for_test_supports() {
        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(make_config()));

        assert_eq!(
            expected,
            GitLabDriver::supports(io, config, url, false).unwrap()
        );
    }
}

fn data_for_test_supports() -> Vec<(&'static str, bool)> {
    let openssl = extension_loaded("openssl");
    vec![
        ("http://gitlab.com/foo/bar", true),
        ("http://gitlab.mycompany.com:5443/foo/bar", true),
        ("http://gitlab.com/foo/bar/", true),
        ("http://gitlab.com/foo/bar/", true),
        ("http://gitlab.com/foo/bar.git", true),
        ("http://gitlab.com/foo/bar.git", true),
        ("http://gitlab.com/foo/bar.baz.git", true),
        ("https://gitlab.com/foo/bar", openssl),
        ("https://gitlab.mycompany.com:5443/foo/bar", openssl),
        ("git@gitlab.com:foo/bar.git", openssl),
        ("git@example.com:foo/bar.git", false),
        ("http://example.com/foo/bar", false),
        ("http://mycompany.com/gitlab/mygroup/myproject", true),
        ("https://mycompany.com/gitlab/mygroup/myproject", openssl),
        (
            "http://othercompany.com/nested/gitlab/mygroup/myproject",
            true,
        ),
        (
            "https://othercompany.com/nested/gitlab/mygroup/myproject",
            openssl,
        ),
        (
            "http://gitlab.com/mygroup/mysubgroup/mysubsubgroup/myproject",
            true,
        ),
        (
            "https://gitlab.com/mygroup/mysubgroup/mysubsubgroup/myproject",
            openssl,
        ),
    ]
}

#[test]
#[ignore = "HttpDownloaderMock (getHttpDownloaderMock) and the IOInterface MockObject are not ported"]
fn test_gitlab_sub_directory() {
    todo!()
}

#[test]
#[ignore = "HttpDownloaderMock (getHttpDownloaderMock) and the IOInterface MockObject are not ported"]
fn test_gitlab_sub_group() {
    todo!()
}

#[test]
#[ignore = "HttpDownloaderMock (getHttpDownloaderMock) and the IOInterface MockObject are not ported"]
fn test_gitlab_sub_directory_sub_group() {
    todo!()
}

#[test]
#[ignore = "HttpDownloaderMock (getHttpDownloaderMock) and the IOInterface MockObject are not ported"]
fn test_forwards_options() {
    todo!()
}

#[test]
#[ignore = "HttpDownloaderMock (getHttpDownloaderMock) and the IOInterface MockObject are not ported"]
fn test_protocol_override_repository_url_generation() {
    todo!()
}

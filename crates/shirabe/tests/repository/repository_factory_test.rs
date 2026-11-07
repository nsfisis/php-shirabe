//! ref: composer/tests/Composer/Test/Repository/RepositoryFactoryTest.php sha1:a4ea64d39e67e55bf64855a2a813c5165b284173

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::{IOInterface, NullIO};
use shirabe::repository::RepositoryFactory;
use shirabe::util::HttpDownloader;
use shirabe_php_shim::{PhpMixed, ToPhpString};

#[test]
fn test_manager_with_all_repository_types() {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(
        HttpDownloader::new(
            io.clone(),
            config.clone(),
            shirabe::util::StreamContextOptions::new(),
            true,
        )
        .unwrap(),
    ));

    let manager =
        RepositoryFactory::manager(io, &config, Some(http_downloader), None, None).unwrap();

    let repository_classes: Vec<bstr::BString> =
        manager.__repository_classes().keys().cloned().collect();

    assert_eq!(
        vec![
            "composer",
            "vcs",
            "package",
            "pear",
            "git",
            "bitbucket",
            "git-bitbucket",
            "github",
            "gitlab",
            "svn",
            "fossil",
            "perforce",
            "hg",
            "artifact",
            "path",
        ],
        repository_classes
    );
}

fn generate_repository_name_provider() -> Vec<(
    PhpMixed,
    Vec<(&'static BStr, PhpMixed)>,
    Vec<&'static BStr>,
    &'static BStr,
)> {
    vec![
        (PhpMixed::Int(0), vec![], vec![], BStr::new("0")),
        (
            PhpMixed::Int(0),
            vec![],
            vec![BStr::new("0")],
            BStr::new("02"),
        ),
        (
            PhpMixed::Int(0),
            vec![(
                BStr::new("url"),
                PhpMixed::string(BString::from("https://example.org")),
            )],
            vec![],
            BStr::new("example.org"),
        ),
        (
            PhpMixed::Int(0),
            vec![(
                BStr::new("url"),
                PhpMixed::string(BString::from("https://example.org")),
            )],
            vec![BStr::new("example.org")],
            BStr::new("example.org2"),
        ),
        (
            PhpMixed::string(BString::from("example.org")),
            vec![(
                BStr::new("url"),
                PhpMixed::string(BString::from("https://example.org/repository")),
            )],
            vec![],
            BStr::new("example.org"),
        ),
        (
            PhpMixed::string(BString::from("example.org")),
            vec![(
                BStr::new("url"),
                PhpMixed::string(BString::from("https://example.org/repository")),
            )],
            vec![BStr::new("example.org")],
            BStr::new("example.org2"),
        ),
    ]
}

#[test]
fn test_generate_repository_name() {
    for (index, repo_pairs, existing_keys, expected) in generate_repository_name_provider() {
        let repo: IndexMap<BString, PhpMixed> = repo_pairs
            .into_iter()
            .map(|(k, v)| (k.to_php_string(), v))
            .collect();
        let existing_repos: IndexMap<BString, ()> = existing_keys
            .into_iter()
            .map(|k| (k.to_php_string(), ()))
            .collect();

        assert_eq!(
            expected,
            RepositoryFactory::generate_repository_name(&index, &repo, &existing_repos)
        );
    }
}

//! ref: composer/tests/Composer/Test/Repository/RepositoryFactoryTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::RepositoryFactory;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe_php_shim::PhpMixed;

#[test]
#[ignore]
fn test_manager_with_all_repository_types() {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let config = Rc::new(RefCell::new(Config::new(false, None)));
    let http_downloader = Rc::new(RefCell::new(HttpDownloader::new(
        io.clone(),
        config.clone(),
        IndexMap::new(),
        true,
    )));

    let manager =
        RepositoryFactory::manager(io, &config, Some(http_downloader), None, None).unwrap();

    let repository_classes: Vec<&str> = manager
        .__repository_classes()
        .keys()
        .map(|k| k.as_str())
        .collect();

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
    Vec<(&'static str, PhpMixed)>,
    Vec<&'static str>,
    &'static str,
)> {
    vec![
        (PhpMixed::Int(0), vec![], vec![], "0"),
        (PhpMixed::Int(0), vec![], vec!["0"], "02"),
        (
            PhpMixed::Int(0),
            vec![("url", PhpMixed::String("https://example.org".to_string()))],
            vec![],
            "example.org",
        ),
        (
            PhpMixed::Int(0),
            vec![("url", PhpMixed::String("https://example.org".to_string()))],
            vec!["example.org"],
            "example.org2",
        ),
        (
            PhpMixed::String("example.org".to_string()),
            vec![(
                "url",
                PhpMixed::String("https://example.org/repository".to_string()),
            )],
            vec![],
            "example.org",
        ),
        (
            PhpMixed::String("example.org".to_string()),
            vec![(
                "url",
                PhpMixed::String("https://example.org/repository".to_string()),
            )],
            vec!["example.org"],
            "example.org2",
        ),
    ]
}

#[test]
#[ignore]
fn test_generate_repository_name() {
    for (index, repo_pairs, existing_keys, expected) in generate_repository_name_provider() {
        let repo: IndexMap<String, PhpMixed> = repo_pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let existing_repos: IndexMap<String, ()> = existing_keys
            .into_iter()
            .map(|k| (k.to_string(), ()))
            .collect();

        assert_eq!(
            expected,
            RepositoryFactory::generate_repository_name(&index, &repo, &existing_repos)
        );
    }
}

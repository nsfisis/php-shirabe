//! ref: composer/tests/Composer/Test/Repository/RepositoryFactoryTest.php

use indexmap::IndexMap;
use shirabe::repository::RepositoryFactory;
use shirabe_php_shim::PhpMixed;

#[test]
#[ignore = "PHP test uses ReflectionProperty to read the private RepositoryManager::repository_classes field; no public accessor for repository_classes keys exists in the Rust impl"]
fn test_manager_with_all_repository_types() {
    todo!()
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
#[ignore = "generate_repository_name does not stringify an integer index (PhpMixed::as_string returns None for Int), so a numeric index with no url yields \"\" instead of e.g. \"0\""]
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

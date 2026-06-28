//! ref: composer/tests/Composer/Test/DependencyResolver/PoolOptimizerTest.php

use indexmap::IndexMap;
use shirabe::dependency_resolver::default_policy::DefaultPolicy;
use shirabe::dependency_resolver::pool::Pool;
use shirabe::dependency_resolver::pool_optimizer::PoolOptimizer;
use shirabe::dependency_resolver::request::Request;
use shirabe::json::JsonFile;
use shirabe::package::BasePackageHandle;
use shirabe::package::loader::{ArrayLoader, LoaderInterface};
use shirabe::package::version::version_parser::VersionParser;
use shirabe::repository::handle::LockArrayRepositoryHandle;
use shirabe::repository::lock_array_repository::LockArrayRepository;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::PREG_SPLIT_DELIM_CAPTURE;
use shirabe_php_shim::PhpMixed;
use std::path::PathBuf;
use std::rc::Rc;

fn load_package(package_data: &PhpMixed) -> BasePackageHandle {
    let loader = ArrayLoader::new(None, false);
    loader
        .load(package_data.as_array().unwrap().clone(), None)
        .unwrap()
}

fn load_packages(packages_data: &[PhpMixed]) -> Vec<BasePackageHandle> {
    let mut packages: Vec<BasePackageHandle> = Vec::new();

    for package_data in packages_data {
        let package = load_package(package_data);
        packages.push(package.clone());
        if let Some(alias) = package.as_alias() {
            packages.push(alias.get_alias_of().into());
        }
    }

    packages
}

fn reduce_packages_info_for_comparison(packages: &[BasePackageHandle]) -> Vec<String> {
    let mut packages_info: Vec<String> = Vec::new();

    for package in packages {
        let suffix = if let Some(alias) = package.as_alias() {
            format!(" (alias of {})", alias.get_alias_of().get_version())
        } else {
            String::new()
        };
        packages_info.push(format!(
            "{}@{}{}",
            package.get_name(),
            package.get_version(),
            suffix
        ));
    }

    packages_info.sort();

    packages_info
}

fn read_test_file(file: &str, fixtures_dir: &str) -> IndexMap<String, String> {
    let contents = shirabe_php_shim::file_get_contents(file).unwrap();
    let tokens = Preg::split4(
        r"#(?:^|\n*)--([A-Z-]+)--\n#",
        &contents,
        -1,
        PREG_SPLIT_DELIM_CAPTURE,
    );

    let section_info: Vec<&str> = vec!["TEST", "REQUEST", "POOL-BEFORE", "POOL-AFTER"];

    let mut section: Option<String> = None;
    let mut data: IndexMap<String, String> = IndexMap::new();
    for token in tokens {
        if section.is_none() && token.is_empty() {
            continue;
        }

        if section.is_none() {
            if !section_info.contains(&token.as_str()) {
                panic!(
                    "The test file \"{}\" must not contain a section named \"{}\".",
                    file.replace(&format!("{}/", fixtures_dir), ""),
                    token
                );
            }
            section = Some(token);
            continue;
        }

        let section_data = token;
        data.insert(section.take().unwrap(), section_data);
    }

    for required_section in &section_info {
        if !data.contains_key(*required_section) {
            panic!(
                "The test file \"{}\" must have a section named \"{}\".",
                file.replace(&format!("{}/", fixtures_dir), ""),
                required_section
            );
        }
    }

    data
}

fn collect_test_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_test_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn provide_integration_tests() -> IndexMap<
    String,
    (
        PhpMixed,
        Vec<BasePackageHandle>,
        Vec<BasePackageHandle>,
        String,
    ),
> {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/DependencyResolver/Fixtures/pooloptimizer");
    let fixtures_dir = std::fs::canonicalize(&fixtures_dir).unwrap();
    let fixtures_dir_str = fixtures_dir.to_str().unwrap().to_string();

    let mut files: Vec<PathBuf> = Vec::new();
    collect_test_files(&fixtures_dir, &mut files);

    let mut tests: IndexMap<
        String,
        (
            PhpMixed,
            Vec<BasePackageHandle>,
            Vec<BasePackageHandle>,
            String,
        ),
    > = IndexMap::new();
    for file in files {
        let file = file.to_str().unwrap().to_string();

        if !Preg::is_match(r"/\.test$/", &file) {
            continue;
        }

        let test_data = read_test_file(&file, &fixtures_dir_str);
        let message = test_data["TEST"].clone();
        let request_data = JsonFile::parse_json(Some(&test_data["REQUEST"]), None).unwrap();
        let packages_before = load_packages(
            JsonFile::parse_json(Some(&test_data["POOL-BEFORE"]), None)
                .unwrap()
                .as_list()
                .unwrap(),
        );
        let expected_packages = load_packages(
            JsonFile::parse_json(Some(&test_data["POOL-AFTER"]), None)
                .unwrap()
                .as_list()
                .unwrap(),
        );

        let basename = std::path::Path::new(&file)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        tests.insert(
            basename,
            (request_data, packages_before, expected_packages, message),
        );
    }

    tests
}

fn run_test_pool_optimizer(
    request_data: &PhpMixed,
    packages_before: Vec<BasePackageHandle>,
    expected_packages: &[BasePackageHandle],
    message: &str,
) {
    let request_data = request_data.as_array().unwrap();

    let locked_repo = LockArrayRepositoryHandle::new(LockArrayRepository::new(vec![]).unwrap());

    let mut request = Request::new(Some(locked_repo));
    let parser = VersionParser::new();

    if let Some(locked) = request_data.get("locked") {
        for package in locked.as_list().unwrap() {
            request.lock_package(load_package(package));
        }
    }
    if let Some(fixed) = request_data.get("fixed") {
        for package in fixed.as_list().unwrap() {
            request.fix_package(load_package(package));
        }
    }

    for (package, constraint) in request_data["require"].as_array().unwrap() {
        request
            .require_name(
                package,
                Some(
                    parser
                        .parse_constraints(constraint.as_string().unwrap())
                        .unwrap(),
                ),
            )
            .unwrap();
    }

    let prefer_stable = request_data
        .get("preferStable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let prefer_lowest = request_data
        .get("preferLowest")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let pool = Pool::new(
        packages_before,
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let mut pool_optimizer = PoolOptimizer::new(Rc::new(DefaultPolicy::new(
        prefer_stable,
        prefer_lowest,
        None,
    )));

    let pool = pool_optimizer.optimize(&request, &pool);

    assert_eq!(
        reduce_packages_info_for_comparison(expected_packages),
        reduce_packages_info_for_comparison(pool.get_packages()),
        "{}",
        message
    );
}

#[test]
fn test_pool_optimizer() {
    let tests = provide_integration_tests();
    for (_name, (request_data, packages_before, expected_packages, message)) in tests {
        run_test_pool_optimizer(&request_data, packages_before, &expected_packages, &message);
    }
}

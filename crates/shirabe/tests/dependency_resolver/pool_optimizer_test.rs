//! ref: composer/tests/Composer/Test/DependencyResolver/PoolOptimizerTest.php sha1:03d43302268d6664f350b63398ce191085833df9

use bstr::{B, BStr, BString, ByteSlice};
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
use shirabe_php_shim::{
    PhpMixed, bconcat, file_get_contents, path_to_bytes, php_regex, preg_is_match,
    preg_split_delim_capture, str_replace,
};
use std::path::PathBuf;

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

fn reduce_packages_info_for_comparison(packages: &[BasePackageHandle]) -> Vec<BString> {
    let mut packages_info: Vec<BString> = Vec::new();

    for package in packages {
        let suffix = if let Some(alias) = package.as_alias() {
            bconcat!(" (alias of ", alias.get_alias_of().get_version(), ")")
        } else {
            BString::default()
        };
        packages_info.push(bconcat!(
            package.get_name(),
            "@",
            package.get_version(),
            suffix
        ));
    }

    packages_info.sort();

    packages_info
}

fn read_test_file(
    file: &std::path::Path,
    fixtures_dir: &std::path::Path,
) -> IndexMap<BString, BString> {
    let contents = file_get_contents(file).unwrap();
    let tokens = preg_split_delim_capture(php_regex!(r"#(?:^|\n*)--([A-Z-]+)--\n#"), &contents);

    let section_info: Vec<&BStr> = vec![
        BStr::new("TEST"),
        BStr::new("REQUEST"),
        BStr::new("POOL-BEFORE"),
        BStr::new("POOL-AFTER"),
    ];

    let mut section: Option<BString> = None;
    let mut data: IndexMap<BString, BString> = IndexMap::new();
    for token in tokens {
        if section.is_none() && token.is_empty() {
            continue;
        }

        if section.is_none() {
            if !section_info.iter().any(|name| *name == token) {
                panic!(
                    "The test file \"{}\" must not contain a section named \"{}\".",
                    str_replace(bconcat!(fixtures_dir, "/"), "", path_to_bytes(file))
                        .display_lossy(),
                    token.display_lossy()
                );
            }
            section = Some(token);
            continue;
        }

        data.insert(section.take().unwrap(), token);
    }

    for required_section in &section_info {
        if !data.contains_key(B(required_section)) {
            panic!(
                "The test file \"{}\" must have a section named \"{}\".",
                str_replace(bconcat!(fixtures_dir, "/"), "", path_to_bytes(file)).display_lossy(),
                required_section.display_lossy()
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
    BString,
    (
        PhpMixed,
        Vec<BasePackageHandle>,
        Vec<BasePackageHandle>,
        BString,
    ),
> {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/DependencyResolver/Fixtures/pooloptimizer");
    let fixtures_dir = std::fs::canonicalize(&fixtures_dir).unwrap();

    let mut files: Vec<PathBuf> = Vec::new();
    collect_test_files(&fixtures_dir, &mut files);

    let mut tests: IndexMap<
        BString,
        (
            PhpMixed,
            Vec<BasePackageHandle>,
            Vec<BasePackageHandle>,
            BString,
        ),
    > = IndexMap::new();
    for file in files {
        if !preg_is_match(php_regex!(r"/\.test$/"), path_to_bytes(&file)) {
            continue;
        }

        let test_data = read_test_file(&file, &fixtures_dir);
        let message = test_data[B("TEST")].clone();
        let request_data = JsonFile::parse_json(Some(&test_data[B("REQUEST")]), None).unwrap();
        let packages_before = load_packages(
            JsonFile::parse_json(Some(&test_data[B("POOL-BEFORE")]), None)
                .unwrap()
                .as_list()
                .unwrap(),
        );
        let expected_packages = load_packages(
            JsonFile::parse_json(Some(&test_data[B("POOL-AFTER")]), None)
                .unwrap()
                .as_list()
                .unwrap(),
        );

        let basename = BString::from(path_to_bytes(file.file_name().unwrap()));
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
    message: &[u8],
) {
    let request_data = request_data.as_array().unwrap();

    let locked_repo = LockArrayRepositoryHandle::new(LockArrayRepository::new(vec![]).unwrap());

    let mut request = Request::new(Some(locked_repo));
    let parser = VersionParser::new();

    if let Some(locked) = request_data.get(B("locked")) {
        for package in locked.as_list().unwrap() {
            request.lock_package(load_package(package));
        }
    }
    if let Some(fixed) = request_data.get(B("fixed")) {
        for package in fixed.as_list().unwrap() {
            request.fix_package(load_package(package));
        }
    }

    for (package, constraint) in request_data[B("require")].as_array().unwrap() {
        request
            .require_name(
                package.as_bstr(),
                Some(
                    parser
                        .parse_constraints(constraint.as_string().unwrap())
                        .unwrap(),
                ),
            )
            .unwrap();
    }

    let prefer_stable = request_data
        .get(B("preferStable"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let prefer_lowest = request_data
        .get(B("preferLowest"))
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
    let mut pool_optimizer = PoolOptimizer::new(std::rc::Rc::new(DefaultPolicy::new(
        prefer_stable,
        prefer_lowest,
        None,
    )));

    let pool = pool_optimizer.optimize(&request, &pool);

    assert_eq!(
        reduce_packages_info_for_comparison(expected_packages),
        reduce_packages_info_for_comparison(pool.get_packages()),
        "{}",
        message.as_bstr().display_lossy()
    );
}

#[test]
fn test_pool_optimizer() {
    let tests = provide_integration_tests();
    for (_name, (request_data, packages_before, expected_packages, message)) in tests {
        run_test_pool_optimizer(&request_data, packages_before, &expected_packages, &message);
    }
}

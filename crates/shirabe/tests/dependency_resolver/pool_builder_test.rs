//! ref: composer/tests/Composer/Test/DependencyResolver/PoolBuilderTest.php

use bstr::{B, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::dependency_resolver::default_policy::DefaultPolicy;
use shirabe::dependency_resolver::pool::Pool;
use shirabe::dependency_resolver::pool_optimizer::PoolOptimizer;
use shirabe::dependency_resolver::request::{Request, UpdateAllowTransitiveDeps};
use shirabe::io::io_interface::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::json::JsonFile;
use shirabe::package::loader::{ArrayLoader, LoaderInterface};
use shirabe::package::version::version_parser::VersionParser;
use shirabe::package::{BasePackageHandle, STABILITIES};
use shirabe::repository::array_repository::ArrayRepository;
use shirabe::repository::filter_repository::FilterRepository;
use shirabe::repository::handle::{LockArrayRepositoryHandle, RepositoryInterfaceHandle};
use shirabe::repository::lock_array_repository::LockArrayRepository;
use shirabe::repository::repository_factory::RepositoryFactory;
use shirabe::repository::repository_set::{RepositorySet, RootAliasInput};
use shirabe_php_shim::{
    PhpMixed, file_get_contents, legacy_lossy, php_regex, preg_is_match, preg_split_delim_capture,
};
use std::path::PathBuf;

/// Maps the PHP `$loadPackage` closure: pops the optional `id` from the data, loads the
/// package and records it in `package_ids` keyed by that id (erroring on duplicates).
fn load_package(
    loader: &ArrayLoader,
    package_ids: &mut IndexMap<i64, BasePackageHandle>,
    data: &IndexMap<BString, PhpMixed>,
) -> BasePackageHandle {
    let mut data = data.clone();

    let mut id: Option<i64> = None;
    // PHP: !empty($data['id'])
    if let Some(id_val) = data.get(B("id"))
        && let Some(i) = id_val.as_int()
        && i != 0
    {
        id = Some(i);
        data.shift_remove(B("id"));
    }

    let pkg = loader.load(data, None).unwrap();

    if let Some(id) = id {
        if package_ids.contains_key(&id) {
            panic!("Duplicate package id {} defined", id);
        }
        package_ids.insert(id, pkg.clone());
    }

    pkg
}

fn read_test_file(file: &str, fixtures_dir: &str) -> IndexMap<String, String> {
    // TODO(bytes)
    let contents = String::from_utf8_lossy(&file_get_contents(file).unwrap()).into_owned();
    let tokens = preg_split_delim_capture(php_regex!(r"#(?:^|\n*)--([A-Z-]+)--\n#"), &contents);

    // PHP section_info is a map of name => required flag.
    let section_info: Vec<(&str, bool)> = vec![
        ("TEST", true),
        ("ROOT", false),
        ("REQUEST", true),
        ("FIXED", false),
        ("PACKAGE-REPOS", true),
        ("EXPECT", true),
        ("EXPECT-OPTIMIZED", false),
    ];

    let mut section: Option<String> = None;
    let mut data: IndexMap<String, String> = IndexMap::new();
    for token in tokens {
        if section.is_none() && token.is_empty() {
            continue;
        }

        if section.is_none() {
            if !section_info
                .iter()
                .any(|(name, _)| *name == &*legacy_lossy(&token))
            {
                panic!(
                    "The test file \"{}\" must not contain a section named \"{}\".",
                    file.replace(&format!("{}/", fixtures_dir), ""),
                    token
                );
            }
            section = Some(legacy_lossy(&token).into_owned());
            continue;
        }

        let section_data = legacy_lossy(&token).into_owned();
        data.insert(section.take().unwrap(), section_data);
    }

    for (section, required) in &section_info {
        if *required && !data.contains_key(*section) {
            panic!(
                "The test file \"{}\" must have a section named \"{}\".",
                file.replace(&format!("{}/", fixtures_dir), ""),
                section
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

struct IntegrationTest {
    file: String,
    message: String,
    expect: PhpMixed,
    expect_optimized: PhpMixed,
    root: PhpMixed,
    request: PhpMixed,
    package_repos: PhpMixed,
    fixed: PhpMixed,
}

fn get_integration_tests(fixtures_dir: &std::path::Path) -> IndexMap<String, IntegrationTest> {
    let fixtures_dir_str = fixtures_dir.to_str().unwrap().to_string();

    let mut files: Vec<PathBuf> = Vec::new();
    collect_test_files(fixtures_dir, &mut files);

    let mut tests: IndexMap<String, IntegrationTest> = IndexMap::new();
    for file in files {
        let file = file.to_str().unwrap().to_string();

        if !preg_is_match(php_regex!(r"/\.test$/"), &file) {
            continue;
        }

        let test_data = read_test_file(&file, &fixtures_dir_str);

        let message = test_data["TEST"].clone();

        let request = JsonFile::parse_json(Some(&test_data["REQUEST"]), None).unwrap();
        // PHP: !empty($testData['ROOT']) ? parseJson(...) : []
        let root = match test_data.get("ROOT") {
            Some(s) if !s.is_empty() => JsonFile::parse_json(Some(s), None).unwrap(),
            _ => PhpMixed::List(vec![]),
        };

        let package_repos = JsonFile::parse_json(Some(&test_data["PACKAGE-REPOS"]), None).unwrap();
        let fixed = match test_data.get("FIXED") {
            Some(s) if !s.is_empty() => JsonFile::parse_json(Some(s), None).unwrap(),
            _ => PhpMixed::List(vec![]),
        };
        let expect = JsonFile::parse_json(Some(&test_data["EXPECT"]), None).unwrap();
        let expect_optimized = match test_data.get("EXPECT-OPTIMIZED") {
            Some(s) if !s.is_empty() => JsonFile::parse_json(Some(s), None).unwrap(),
            _ => expect.clone(),
        };

        let basename = std::path::Path::new(&file)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        tests.insert(
            basename,
            IntegrationTest {
                file: file.replace(&format!("{}/", fixtures_dir_str), ""),
                message,
                expect,
                expect_optimized,
                root,
                request,
                package_repos,
                fixed,
            },
        );
    }

    tests
}

/// PHP `sort()` with SORT_REGULAR over a mixed int/string list. Mirrors PHP 8 loose
/// comparison: two ints compare numerically; a string and an int that looks numeric
/// compare numerically; otherwise both are compared as strings.
fn php_sort_mixed(values: &mut [PhpMixed]) {
    fn cmp(a: &PhpMixed, b: &PhpMixed) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let num = |m: &PhpMixed| -> Option<f64> {
            match m {
                PhpMixed::Int(i) => Some(*i as f64),
                PhpMixed::Float(f) => Some(*f),
                PhpMixed::String(s) => s.to_str_lossy().trim().parse::<f64>().ok(),
                _ => None,
            }
        };
        let str_of = |m: &PhpMixed| -> String {
            match m {
                PhpMixed::Int(i) => i.to_string(),
                PhpMixed::Float(f) => f.to_string(),
                PhpMixed::String(s) => s.to_str_lossy().into_owned(),
                _ => String::new(),
            }
        };
        match (a, b) {
            (PhpMixed::Int(x), PhpMixed::Int(y)) => x.cmp(y),
            _ => match (num(a), num(b)) {
                (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
                _ => str_of(a).cmp(&str_of(b)),
            },
        }
    }
    values.sort_by(cmp);
}

/// ref: PoolBuilderTest::getPackageResultSet
fn get_package_result_set(
    pool: &Pool,
    package_ids: &IndexMap<i64, BasePackageHandle>,
) -> Vec<PhpMixed> {
    let mut result: Vec<BasePackageHandle> = Vec::new();
    let count = pool.__count();
    // PHP: for ($i = 1; $i <= $count; $i++)
    let mut i = 1;
    while i <= count {
        result.push(pool.package_by_id(i));
        i += 1;
    }

    // PHP: array_search($package, $packageIds, true) — identity lookup.
    let find_id = |package: &BasePackageHandle| -> Option<i64> {
        for (id, candidate) in package_ids {
            if candidate.ptr_id() == package.ptr_id() {
                return Some(*id);
            }
        }
        None
    };

    result
        .into_iter()
        .map(|package| {
            // PHP: if ($id = array_search(...)) — id keys start at 1 so always truthy when found.
            if let Some(id) = find_id(&package) {
                return PhpMixed::Int(id);
            }

            let mut suffix = String::new();
            if let Some(source_reference) = package.get_source_reference() {
                suffix = format!("#{}", source_reference);
            }
            if let Some(repo) = package.get_repository()
                && repo.is::<LockArrayRepository>()
            {
                suffix.push_str(" (locked)");
            }

            if let Some(alias) = package.as_alias() {
                let alias_of = alias.get_alias_of();
                // PHP: array_search($package->getAliasOf(), $packageIds, true)
                let mut matched: Option<i64> = None;
                for (id, candidate) in package_ids {
                    if candidate.ptr_id() == alias_of.ptr_id() {
                        matched = Some(*id);
                        break;
                    }
                }
                if let Some(id) = matched {
                    return PhpMixed::string(format!(
                        "{}-{}{} (alias of {})",
                        package.get_name(),
                        package.get_version(),
                        suffix,
                        id
                    ));
                }

                return PhpMixed::string(format!(
                    "{}-{}{} (alias of {})",
                    package.get_name(),
                    package.get_version(),
                    suffix,
                    alias_of.get_version()
                ));
            }

            PhpMixed::string(format!(
                "{}-{}{}",
                package.get_name(),
                package.get_version(),
                suffix
            ))
        })
        .collect()
}

/// ref: PoolBuilderTest::testPoolBuilder
#[allow(clippy::too_many_arguments)]
fn run_test_pool_builder(
    fixtures_dir: &std::path::Path,
    _file: &str,
    message: &str,
    expect: &PhpMixed,
    expect_optimized: &PhpMixed,
    root: &PhpMixed,
    request_data: &PhpMixed,
    package_repos: &PhpMixed,
    fixed: &PhpMixed,
) {
    // PHP: $root is array; empty checks against keys.
    let root_map = root.as_array();
    let get_root = |key: &str| -> Option<&PhpMixed> { root_map.and_then(|m| m.get(B(key))) };
    let is_empty = |v: Option<&PhpMixed>| -> bool {
        match v {
            None => true,
            Some(PhpMixed::Null) => true,
            Some(PhpMixed::Bool(false)) => true,
            Some(PhpMixed::Int(0)) => true,
            Some(PhpMixed::String(s)) => s.is_empty(),
            Some(PhpMixed::List(l)) => l.is_empty(),
            Some(PhpMixed::Array(a)) => a.is_empty(),
            Some(PhpMixed::Object(a)) => a.is_empty(),
            _ => false,
        }
    };

    // PHP: $rootAliases = !empty($root['aliases']) ? $root['aliases'] : [];
    let root_aliases_data: Vec<IndexMap<BString, PhpMixed>> = if !is_empty(get_root("aliases")) {
        get_root("aliases")
            .and_then(|v| v.as_list())
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_array().cloned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    let minimum_stability: String = if !is_empty(get_root("minimum-stability")) {
        get_root("minimum-stability")
            .and_then(|v| v.as_string())
            .unwrap_or("stable".into())
            .to_string()
    } else {
        "stable".to_string()
    };

    // PHP: $stabilityFlags map of name => stability string.
    let stability_flags_data: IndexMap<String, String> = if !is_empty(get_root("stability-flags")) {
        get_root("stability-flags")
            .and_then(|v| v.as_array())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| {
                        v.as_string()
                            .map(|s| (k.to_str_lossy().into_owned(), s.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        IndexMap::new()
    };

    let root_references: IndexMap<String, String> = if !is_empty(get_root("references")) {
        get_root("references")
            .and_then(|v| v.as_array())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| {
                        v.as_string()
                            .map(|s| (k.to_str_lossy().into_owned(), s.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        IndexMap::new()
    };

    // PHP: array_map over stability flags, mapping to BasePackage::STABILITIES[$stability].
    let mut stability_flags: IndexMap<String, i64> = IndexMap::new();
    for (name, stability) in &stability_flags_data {
        let Some(value) = STABILITIES.get(stability.as_str()) else {
            panic!("Invalid stability given: {}", stability);
        };
        stability_flags.insert(name.clone(), *value);
    }

    let parser = VersionParser::new();
    // PHP: foreach ($rootAliases as $index => $alias) { normalize version + alias }
    let mut root_aliases: Vec<RootAliasInput> = Vec::new();
    for alias in &root_aliases_data {
        let package = alias
            .get(B("package"))
            .and_then(|v| v.as_string())
            .unwrap_or_default()
            .to_string();
        let version = parser
            .normalize(
                &alias
                    .get(B("version"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default()
                    .to_str_lossy(),
                None,
            )
            .unwrap();
        let alias_str = alias
            .get(B("alias"))
            .and_then(|v| v.as_string())
            .unwrap_or_default()
            .to_string();
        let alias_normalized = parser
            .normalize(
                &alias
                    .get(B("alias"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default()
                    .to_str_lossy(),
                None,
            )
            .unwrap();
        root_aliases.push(RootAliasInput {
            package,
            version,
            alias: alias_str,
            alias_normalized,
        });
    }

    let loader = ArrayLoader::new(None, true);
    let mut package_ids: IndexMap<i64, BasePackageHandle> = IndexMap::new();

    // PHP: $oldCwd = Platform::getCwd(); chdir(__DIR__.'/Fixtures/poolbuilder/');
    let old_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(fixtures_dir).unwrap();

    let mut repository_set = RepositorySet::new(
        &minimum_stability,
        stability_flags,
        root_aliases,
        root_references,
        IndexMap::new(),
        IndexMap::new(),
    );
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut rm = RepositoryFactory::manager(io.clone(), &config, None, None, None).unwrap();

    // PHP: foreach ($packageRepos as $packages)
    for repo_entry in package_repos.as_list().unwrap() {
        // PHP: isset($packages['type'])
        if let Some(repo_map) = repo_entry.as_array()
            && repo_map.contains_key(B("type"))
        {
            let repo = RepositoryFactory::create_repo(
                io.clone(),
                &config,
                repo_map.clone(),
                Some(&mut rm),
            )
            .unwrap();
            repository_set.add_repository(repo).unwrap();
            continue;
        }

        let repo = ArrayRepository::new(vec![]).unwrap();
        let repo_handle = RepositoryInterfaceHandle::new(repo);

        // PHP: isset($packages['canonical']) || isset($packages['only']) || isset($packages['exclude'])
        let packages_list: Vec<IndexMap<BString, PhpMixed>>;
        if let Some(repo_map) = repo_entry.as_array() {
            if repo_map.contains_key(B("canonical"))
                || repo_map.contains_key(B("only"))
                || repo_map.contains_key(B("exclude"))
            {
                let mut options = repo_map.clone();
                let packages = options
                    .shift_remove(B("packages"))
                    .unwrap_or(PhpMixed::List(vec![]));
                repository_set
                    .add_repository(RepositoryInterfaceHandle::new(
                        FilterRepository::new(repo_handle.clone(), options).unwrap(),
                    ))
                    .unwrap();
                packages_list = packages
                    .as_list()
                    .map(|l| l.iter().filter_map(|v| v.as_array().cloned()).collect())
                    .unwrap_or_default();
            } else {
                repository_set.add_repository(repo_handle.clone()).unwrap();
                packages_list = repo_map
                    .values()
                    .filter_map(|v| v.as_array().cloned())
                    .collect();
            }
        } else {
            repository_set.add_repository(repo_handle.clone()).unwrap();
            packages_list = repo_entry
                .as_list()
                .map(|l| l.iter().filter_map(|v| v.as_array().cloned()).collect())
                .unwrap_or_default();
        }

        let array_repo = repo_handle.downcast_rc::<ArrayRepository>().unwrap();
        for package in &packages_list {
            array_repo
                .borrow()
                .add_package(load_package(&loader, &mut package_ids, package))
                .unwrap();
        }
    }

    let locked_repo = LockArrayRepositoryHandle::new(LockArrayRepository::new(vec![]).unwrap());
    repository_set
        .add_repository(locked_repo.clone().into())
        .unwrap();

    let request_map = request_data.as_array().unwrap();

    // PHP: if (isset($requestData['locked']))
    if let Some(locked) = request_map.get(B("locked")) {
        for package in locked.as_list().unwrap() {
            locked_repo
                .borrow()
                .add_package(load_package(
                    &loader,
                    &mut package_ids,
                    package.as_array().unwrap(),
                ))
                .unwrap();
        }
    }

    let mut request = Request::new(Some(locked_repo));
    for (package, constraint) in request_map[B("require")].as_array().unwrap() {
        request
            .require_name(
                &package.to_str_lossy(),
                Some(
                    parser
                        .parse_constraints(&constraint.as_string().unwrap().to_str_lossy())
                        .unwrap(),
                ),
            )
            .unwrap();
    }

    // PHP: if (isset($requestData['allowList']))
    if let Some(allow_list) = request_map.get(B("allowList")) {
        let mut transitive_deps = UpdateAllowTransitiveDeps::UpdateOnlyListed;
        if request_map
            .get(B("allowTransitiveDepsNoRootRequire"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            transitive_deps =
                UpdateAllowTransitiveDeps::UpdateListedWithTransitiveDepsNoRootRequire;
        }
        if request_map
            .get(B("allowTransitiveDeps"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            transitive_deps = UpdateAllowTransitiveDeps::UpdateListedWithTransitiveDeps;
        }
        let allow_list_names: Vec<String> = allow_list
            .as_list()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_string().map(|s| s.to_string()))
            .collect();
        request.set_update_allow_list(allow_list_names, transitive_deps);
    }

    // PHP: foreach ($fixed as $fixedPackage)
    for fixed_package in fixed.as_list().unwrap() {
        request.fix_package(load_package(
            &loader,
            &mut package_ids,
            fixed_package.as_array().unwrap(),
        ));
    }

    let pool = repository_set
        .create_pool(&mut request, io.clone(), None, None, vec![], None, None)
        .unwrap();

    let mut result = get_package_result_set(&pool, &package_ids);

    let mut expect = expect.as_list().unwrap().clone();
    php_sort_mixed(&mut expect);
    php_sort_mixed(&mut result);
    assert_eq!(
        expect, result,
        "Unoptimized pool does not match expected package set ({})",
        message
    );

    let mut optimizer =
        PoolOptimizer::new(std::rc::Rc::new(DefaultPolicy::new(false, false, None)));
    let optimized = optimizer.optimize(&request, &pool);
    let mut result = get_package_result_set(&optimized, &package_ids);
    let mut expect_optimized = expect_optimized.as_list().unwrap().clone();
    php_sort_mixed(&mut expect_optimized);
    php_sort_mixed(&mut result);
    assert_eq!(
        expect_optimized, result,
        "Optimized pool does not match expected package set ({})",
        message
    );

    // PHP: chdir($oldCwd);
    std::env::set_current_dir(&old_cwd).unwrap();
}

#[test]
fn test_pool_builder() {
    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/DependencyResolver/Fixtures/poolbuilder");
    let fixtures_dir = std::fs::canonicalize(&fixtures_dir).unwrap();

    let tests = get_integration_tests(&fixtures_dir);
    for (_name, test) in tests {
        run_test_pool_builder(
            &fixtures_dir,
            &test.file,
            &test.message,
            &test.expect,
            &test.expect_optimized,
            &test.root,
            &test.request,
            &test.package_repos,
            &test.fixed,
        );
    }
}

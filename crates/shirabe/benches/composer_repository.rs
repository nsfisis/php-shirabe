//! Benchmarks for `ComposerRepository::load_packages` against a v2 (metadata-url) repository.
//!
//! The responses are real packagist p2 files served through the HTTP downloader mock, so one
//! iteration covers what `composer update` does per package: decode the response, expand the
//! minified metadata, filter by constraint and stability, and build the accepted packages. The
//! constraint decides how many versions survive that filter, which is what the lazy expansion is
//! meant to exploit.

use criterion::BatchSize;
use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::json::json_file::JsonFile;
use shirabe::package::base_package::STABILITY_STABLE;
use shirabe::package::version::VersionParser;
use shirabe::repository::composer_repository::ComposerRepository;
use shirabe::util::http_downloader::{
    HttpDownloader, HttpDownloaderMockExpectation, HttpDownloaderMockHandler,
};
use shirabe_php_shim::PhpMixed;
use shirabe_semver::constraint::AnyConstraint;
use tempfile::TempDir;

#[path = "packagist_fixture.rs"]
mod packagist_fixture;

const PACKAGES: &[&str] = &["symfony/console", "laravel/framework"];

/// Constraints spanning the acceptance rates a repository sees, from one no version satisfies to
/// one that takes every version there is. Each case is labelled with how many packages survived,
/// since that rate is what decides whether deferring the copy pays off.
const CONSTRAINTS: &[(&str, &str)] = &[
    ("none", "^99.0"),
    ("major7", "^7.0"),
    ("major6-up", ">=6.0"),
    ("major4-up", ">=4.0"),
    ("major2-up", ">=2.0"),
    ("any", "*"),
];

const REPOSITORY_URL: &str = "https://example.org/packages.json";
const METADATA_URL: &str = "https://example.org/p2/%package%.json";

/// A root file with nothing but a metadata-url, which is what puts the repository on the lazy
/// provider path `load_async_packages` serves.
fn root_file() -> String {
    let mut root: IndexMap<String, PhpMixed> = IndexMap::new();
    root.insert("packages".to_string(), PhpMixed::Array(IndexMap::new()));
    root.insert(
        "metadata-url".to_string(),
        PhpMixed::String(METADATA_URL.to_string()),
    );
    JsonFile::encode(&PhpMixed::Array(root)).expect("failed to encode the root file")
}

/// A config whose cache is read-only, so the measurement stays off the filesystem.
fn config(home: &TempDir) -> Config {
    let mut config = Config::new(true, None);
    let mut settings: IndexMap<String, PhpMixed> = IndexMap::new();
    settings.insert(
        "home".to_string(),
        PhpMixed::String(home.path().display().to_string()),
    );
    settings.insert("cache-read-only".to_string(), PhpMixed::Bool(true));
    let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
    top.insert("config".to_string(), PhpMixed::Array(settings));
    config.merge(&top, Config::SOURCE_UNKNOWN);
    config
}

/// A repository answering exactly two requests, in the order `load_packages` makes them: the root
/// file, then the package's metadata.
fn repository(package: &str, root: &str, metadata: &str, config: &Config) -> ComposerRepository {
    let expectations = vec![
        HttpDownloaderMockExpectation {
            url: REPOSITORY_URL.to_string(),
            options: None,
            status: 200,
            body: root.to_string(),
            headers: vec![String::new()],
        },
        HttpDownloaderMockExpectation {
            url: METADATA_URL.replace("%package%", package),
            options: None,
            status: 200,
            body: metadata.to_string(),
            headers: vec![String::new()],
        },
    ];

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(HttpDownloader::__new_mock(
        io.clone(),
        std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None))),
    )));
    http_downloader.borrow_mut().__expects(
        expectations,
        true,
        HttpDownloaderMockHandler::default(),
    );

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".to_string(),
        PhpMixed::String(REPOSITORY_URL.to_string()),
    );

    ComposerRepository::new(repo_config, io, config, http_downloader, None)
        .expect("failed to build the repository")
}

fn load(
    repository: &mut ComposerRepository,
    package: &str,
    constraint: &AnyConstraint,
    acceptable_stabilities: &IndexMap<String, i64>,
) -> usize {
    let mut package_name_map: IndexMap<String, Option<AnyConstraint>> = IndexMap::new();
    package_name_map.insert(package.to_string(), Some(constraint.clone()));
    repository
        .load_packages(
            package_name_map,
            acceptable_stabilities.clone(),
            IndexMap::new(),
            IndexMap::new(),
        )
        .expect("load_packages failed")
        .packages
        .len()
}

/// How many versions the p2 file carries, for the `<loaded>of<total>` labels.
fn version_count(package: &str, metadata: &str) -> usize {
    JsonFile::parse_json(Some(metadata), None)
        .expect("invalid fixture JSON")
        .as_array()
        .and_then(|response| response.get("packages"))
        .and_then(|v| v.as_array())
        .and_then(|packages| packages.get(package))
        .and_then(|v| v.as_list())
        .expect("the fixture has no version list for the package")
        .len()
}

fn bench_load_packages(c: &mut Criterion) {
    let home = TempDir::new().expect("failed to create the config home");
    let config = config(&home);
    let root = root_file();
    let version_parser = VersionParser::new();

    let mut acceptable_stabilities: IndexMap<String, i64> = IndexMap::new();
    acceptable_stabilities.insert("stable".to_string(), STABILITY_STABLE);

    for package in PACKAGES {
        let metadata = packagist_fixture::fetch(package);
        let total = version_count(package, &metadata);
        let mut group = c.benchmark_group(format!("load_packages/{package}"));

        for (label, constraint) in CONSTRAINTS {
            let constraint: AnyConstraint = version_parser
                .parse_constraints(constraint)
                .expect("failed to parse the constraint");

            let mut probe = repository(package, &root, &metadata, &config);
            let loaded = load(&mut probe, package, &constraint, &acceptable_stabilities);
            let parameter = format!("{label}/{loaded}of{total}");

            group.bench_function(BenchmarkId::from_parameter(parameter), |b| {
                b.iter_batched(
                    || repository(package, &root, &metadata, &config),
                    |mut repository| {
                        load(
                            &mut repository,
                            package,
                            &constraint,
                            &acceptable_stabilities,
                        )
                    },
                    BatchSize::PerIteration,
                );
            });
        }

        group.finish();
    }
}

criterion_group!(benches, bench_load_packages);
criterion_main!(benches);

//! Benchmarks for `ComposerRepository::load_packages` against a v2 (metadata-url) repository.
//!
//! The responses are real packagist p2 files served through the HTTP downloader mock, so one
//! iteration covers what `composer update` does per package: decode the response, expand the
//! minified metadata, filter by constraint and stability, and build the accepted packages. The
//! constraint decides how many versions survive that filter, which is what the lazy expansion is
//! meant to exploit.

use bstr::{BStr, BString, ByteSlice};
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::{IOInterface, NullIO};
use shirabe::json::JsonFile;
use shirabe::package::STABILITY_STABLE;
use shirabe::package::version::VersionParser;
use shirabe::repository::ComposerRepository;
use shirabe::util::{HttpDownloader, HttpDownloaderMockExpectation, HttpDownloaderMockHandler};
use shirabe_php_shim::{PhpMixed, path_to_bytes, php_str, str_replace};
use shirabe_semver::constraint::AnyConstraint;
use tempfile::TempDir;

#[path = "packagist_fixture.rs"]
mod packagist_fixture;

const PACKAGES: &[&BStr] = &[php_str(b"symfony/console"), php_str(b"laravel/framework")];

/// Constraints spanning the acceptance rates a repository sees, from one no version satisfies to
/// one that takes every version there is. Each case is labelled with how many packages survived,
/// since that rate is what decides whether deferring the copy pays off.
const CONSTRAINTS: &[(&BStr, &BStr)] = &[
    (php_str(b"none"), php_str(b"^99.0")),
    (php_str(b"major7"), php_str(b"^7.0")),
    (php_str(b"major6-up"), php_str(b">=6.0")),
    (php_str(b"major4-up"), php_str(b">=4.0")),
    (php_str(b"major2-up"), php_str(b">=2.0")),
    (php_str(b"any"), php_str(b"*")),
];

const REPOSITORY_URL: &BStr = php_str(b"https://example.org/packages.json");
const METADATA_URL: &BStr = php_str(b"https://example.org/p2/%package%.json");

/// A root file with nothing but a metadata-url, which is what puts the repository on the lazy
/// provider path `load_async_packages` serves.
fn root_file() -> BString {
    let mut root: IndexMap<BString, PhpMixed> = IndexMap::new();
    root.insert("packages".into(), PhpMixed::Array(IndexMap::new()));
    root.insert("metadata-url".into(), PhpMixed::string(METADATA_URL));
    JsonFile::encode(&PhpMixed::Array(root)).expect("failed to encode the root file")
}

/// A config whose cache is read-only, so the measurement stays off the filesystem.
fn config(home: &TempDir) -> Config {
    let mut config = Config::new(true, None);
    let mut settings: IndexMap<BString, PhpMixed> = IndexMap::new();
    settings.insert("home".into(), PhpMixed::string(path_to_bytes(home.path())));
    settings.insert("cache-read-only".into(), PhpMixed::Bool(true));
    let mut top: IndexMap<BString, PhpMixed> = IndexMap::new();
    top.insert("config".into(), PhpMixed::Array(settings));
    config.merge(&top, Config::SOURCE_UNKNOWN);
    config
}

/// A repository answering exactly two requests, in the order `load_packages` makes them: the root
/// file, then the package's metadata.
fn repository(
    package: &BStr,
    root: &bstr::BStr,
    metadata: &BStr,
    config: &Config,
) -> ComposerRepository {
    let expectations = vec![
        HttpDownloaderMockExpectation {
            url: REPOSITORY_URL.into(),
            options: None,
            status: 200,
            body: root.into(),
            headers: vec![bstr::BString::default()],
        },
        HttpDownloaderMockExpectation {
            url: str_replace("%package%", package, METADATA_URL),
            options: None,
            status: 200,
            body: metadata.into(),
            headers: vec![bstr::BString::default()],
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

    let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
    repo_config.insert("url".into(), PhpMixed::string(REPOSITORY_URL));

    ComposerRepository::new(repo_config, io, config, http_downloader, None)
        .expect("failed to build the repository")
}

fn load(
    repository: &mut ComposerRepository,
    package: &BStr,
    constraint: &AnyConstraint,
    acceptable_stabilities: &IndexMap<bstr::BString, i64>,
) -> usize {
    let mut package_name_map: IndexMap<bstr::BString, Option<AnyConstraint>> = IndexMap::new();
    package_name_map.insert(package.into(), Some(constraint.clone()));
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
fn version_count(package: &BStr, metadata: &BStr) -> usize {
    JsonFile::parse_json(Some(metadata), None)
        .expect("invalid fixture JSON")
        .as_array()
        .and_then(|response| response.get(bstr::B("packages")))
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

    let mut acceptable_stabilities: IndexMap<bstr::BString, i64> = IndexMap::new();
    acceptable_stabilities.insert("stable".into(), STABILITY_STABLE);

    for package in PACKAGES {
        let metadata = packagist_fixture::fetch(package);
        let total = version_count(package, metadata.as_bstr());
        #[expect(
            clippy::disallowed_macros,
            reason = "criterion names a benchmark with text"
        )]
        let mut group = c.benchmark_group(format!("load_packages/{}", package.display_lossy()));

        for (label, constraint) in CONSTRAINTS {
            let constraint: AnyConstraint = version_parser
                .parse_constraints(constraint)
                .expect("failed to parse the constraint");

            let mut probe = repository(package, root.as_ref(), metadata.as_bstr(), &config);
            let loaded = load(&mut probe, package, &constraint, &acceptable_stabilities);
            #[expect(
                clippy::disallowed_macros,
                reason = "criterion names a benchmark with text"
            )]
            let parameter = format!("{}/{loaded}of{total}", label.display_lossy());

            group.bench_function(BenchmarkId::from_parameter(parameter), |b| {
                b.iter_batched(
                    || repository(package, root.as_ref(), metadata.as_bstr(), &config),
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

//! Benchmarks for expanding minified package metadata.
//!
//! The workload mirrors `ComposerRepository::load_async_packages`: every expanded version is
//! inspected, but only the ones accepted by the constraint and stability filters are kept. The
//! `keep` parameter is that acceptance rate, which is what decides how much of the expansion has
//! to be copied out.

use criterion::BatchSize;
use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::criterion_group;
use criterion::criterion_main;
use indexmap::IndexMap;
use shirabe::json::json_file::JsonFile;
use shirabe_metadata_minifier::MetadataMinifier;
use shirabe_php_shim::PhpMixed;
use std::hint::black_box;

#[path = "packagist_fixture.rs"]
mod packagist_fixture;

/// Real packagist metadata, fetched on first run. `symfony/console` and `laravel/framework` are
/// long-lived packages whose p2 files carry hundreds of versions each.
const PACKAGES: &[&str] = &["monolog/monolog", "symfony/console", "laravel/framework"];

/// Percentages of the expanded versions the caller keeps.
const KEEP_PERCENTS: &[u32] = &[0, 10, 50, 100];

/// The minified version list for `package`, in the shape `load_async_packages` hands to the
/// minifier.
fn minified_versions(package: &str) -> Vec<IndexMap<String, PhpMixed>> {
    let response = JsonFile::parse_json(Some(&packagist_fixture::fetch(package)), None)
        .expect("invalid fixture JSON");
    let response = response.as_array().expect("the fixture is not an object");
    assert_eq!(
        response.get("minified").and_then(|v| v.as_string()),
        Some("composer/2.0"),
        "{package} is not served in the minified format",
    );

    response
        .get("packages")
        .and_then(|v| v.as_array())
        .and_then(|packages| packages.get(package))
        .and_then(|v| v.as_list())
        .expect("the fixture has no version list for the package")
        .iter()
        .map(|version| {
            version
                .as_array()
                .expect("a version is not an object")
                .clone()
        })
        .collect()
}

/// Reads one field of every expanded version and keeps the newest `keep` of them, the way the
/// repository keeps only the versions its filters accept.
fn consume(
    versions: Vec<IndexMap<String, PhpMixed>>,
    keep: usize,
) -> Vec<IndexMap<String, PhpMixed>> {
    let expanded = MetadataMinifier::expand(versions);
    let first_kept = expanded.len() - keep;
    let mut kept: Vec<IndexMap<String, PhpMixed>> = Vec::with_capacity(keep);
    for index in 0..expanded.len() {
        black_box(expanded.version(index).get("version_normalized"));
        if index >= first_kept {
            kept.push(expanded.materialize(index));
        }
    }
    kept
}

fn bench_expand(c: &mut Criterion) {
    for package in PACKAGES {
        let versions = minified_versions(package);
        let mut group = c.benchmark_group(format!("expand/{package}"));

        for keep_percent in KEEP_PERCENTS {
            let keep = versions.len() * (*keep_percent as usize) / 100;
            let parameter = format!("keep{keep_percent}%/{}of{}", keep, versions.len());

            group.bench_function(BenchmarkId::from_parameter(parameter), |b| {
                b.iter_batched(
                    || versions.clone(),
                    |versions| consume(versions, keep),
                    BatchSize::PerIteration,
                );
            });
        }

        group.finish();
    }
}

criterion_group!(benches, bench_expand);
criterion_main!(benches);

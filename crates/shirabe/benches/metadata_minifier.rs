//! Benchmarks for expanding minified package metadata.
//!
//! The workload mirrors `ComposerRepository::load_async_packages`: every expanded version is
//! inspected, but only the ones accepted by the constraint and stability filters are kept. The
//! `keep` parameter is that acceptance rate, which is what decides how much of the expansion has
//! to be copied out.

use bstr::{BStr, BString, ByteSlice};
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use indexmap::IndexMap;
use shirabe::json::JsonFile;
use shirabe_metadata_minifier::MetadataMinifier;
use shirabe_php_shim::{PhpMixed, php_str};
use std::hint::black_box;

#[path = "packagist_fixture.rs"]
mod packagist_fixture;

/// Real packagist metadata, fetched on first run. `symfony/console` and `laravel/framework` are
/// long-lived packages whose p2 files carry hundreds of versions each.
const PACKAGES: &[&BStr] = &[
    php_str(b"monolog/monolog"),
    php_str(b"symfony/console"),
    php_str(b"laravel/framework"),
];

/// Percentages of the expanded versions the caller keeps.
const KEEP_PERCENTS: &[u32] = &[0, 10, 50, 100];

/// The minified version list for `package`, in the shape `load_async_packages` hands to the
/// minifier.
fn minified_versions(package: &BStr) -> Vec<IndexMap<BString, PhpMixed>> {
    let response = JsonFile::parse_json(Some(packagist_fixture::fetch(package).as_bstr()), None)
        .expect("invalid fixture JSON");
    let response = response.as_array().expect("the fixture is not an object");
    assert_eq!(
        response
            .get(bstr::B("minified"))
            .and_then(|v| v.as_string()),
        Some("composer/2.0".into()),
        "{} is not served in the minified format",
        package.display_lossy(),
    );

    response
        .get(bstr::B("packages"))
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
    versions: Vec<IndexMap<BString, PhpMixed>>,
    keep: usize,
) -> Vec<IndexMap<BString, PhpMixed>> {
    let expanded = MetadataMinifier::expand(versions);
    let first_kept = expanded.len() - keep;
    let mut kept: Vec<IndexMap<BString, PhpMixed>> = Vec::with_capacity(keep);
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
        #[expect(
            clippy::disallowed_macros,
            reason = "criterion names a benchmark with text"
        )]
        let mut group = c.benchmark_group(format!("expand/{}", package.display_lossy()));

        for keep_percent in KEEP_PERCENTS {
            let keep = versions.len() * (*keep_percent as usize) / 100;
            #[expect(
                clippy::disallowed_macros,
                reason = "criterion names a benchmark with text"
            )]
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

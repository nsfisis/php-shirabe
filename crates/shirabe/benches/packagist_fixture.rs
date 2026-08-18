//! Real packagist p2 metadata for the benchmarks, fetched on first run and cached under the
//! target directory so later runs work offline.

use std::path::PathBuf;

pub fn fetch(package: &str) -> String {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("packagist-p2");
    std::fs::create_dir_all(&dir).expect("failed to create the fixture directory");
    let path = dir.join(format!("{}.json", package.replace('/', "-")));
    if let Ok(fixture) = std::fs::read_to_string(&path) {
        return fixture;
    }

    let url = format!("https://repo.packagist.org/p2/{package}.json");
    let fixture = reqwest::blocking::get(&url)
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.text())
        .unwrap_or_else(|e| {
            panic!(
                "failed to fetch {url}: {e}; download it to {} by hand to run this benchmark offline",
                path.display(),
            )
        });
    std::fs::write(&path, &fixture).expect("failed to cache the fixture");
    fixture
}

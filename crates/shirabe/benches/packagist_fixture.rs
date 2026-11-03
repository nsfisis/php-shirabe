//! Real packagist p2 metadata for the benchmarks, fetched on first run and cached under the
//! target directory so later runs work offline.

use bstr::{BStr, BString};
use shirabe_php_shim::{bconcat, path_from_bytes, str_replace, utf8_text};
use std::path::PathBuf;

pub fn fetch(package: &BStr) -> BString {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("packagist-p2");
    std::fs::create_dir_all(&dir).expect("failed to create the fixture directory");
    let path = dir.join(path_from_bytes(&bconcat!(
        str_replace("/", "-", package),
        ".json"
    )));
    if let Ok(fixture) = std::fs::read(&path) {
        return fixture.into();
    }

    let url = bconcat!("https://repo.packagist.org/p2/", package, ".json");
    let fixture = reqwest::blocking::get(utf8_text(&url).expect("reqwest takes a text URL"))
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.bytes())
        .unwrap_or_else(|e| {
            panic!(
                "failed to fetch {}: {e}; download it to {} by hand to run this benchmark offline",
                url.display_lossy(),
                path.display(),
            )
        });
    std::fs::write(&path, &fixture).expect("failed to cache the fixture");
    fixture.to_vec().into()
}

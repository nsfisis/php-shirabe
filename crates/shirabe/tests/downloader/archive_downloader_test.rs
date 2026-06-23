//! ref: composer/tests/Composer/Test/Downloader/ArchiveDownloaderTest.php

// The PHP test builds an anonymous ArchiveDownloader subclass; getFileName/processUrl are
// inherited unchanged from FileDownloader, so the concrete FileDownloader is exercised here
// directly. The PHP mocks of IOInterface/Config/PackageInterface are replaced by a NullIO, a
// real Config merged with `vendor-dir`, and real CompletePackage instances with dist
// url/reference set.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::downloader::FileDownloader;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::HttpDownloader;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::VersionParser;

/// ref: TestCase::getPackage (default class CompletePackage)
fn get_package(name: &str, version: &str) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(version, None).unwrap();
    CompletePackageHandle::new(name.to_string(), norm_version, version.to_string()).into()
}

/// ref: ArchiveDownloaderTest::getArchiveDownloaderMock (the inherited getFileName/processUrl
/// live on FileDownloader, so the concrete downloader is built directly).
fn get_archive_downloader(vendor_dir: Option<&str>) -> FileDownloader {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));

    let mut config = Config::new(false, None);
    if let Some(vendor_dir) = vendor_dir {
        let mut config_options: IndexMap<String, PhpMixed> = IndexMap::new();
        config_options.insert(
            "vendor-dir".to_string(),
            PhpMixed::String(vendor_dir.to_string()),
        );
        let mut merged: IndexMap<String, PhpMixed> = IndexMap::new();
        merged.insert("config".to_string(), PhpMixed::Array(config_options));
        config.merge(&merged, "test");
    }
    let config = Rc::new(RefCell::new(config));

    let http_downloader = Rc::new(RefCell::new(HttpDownloader::new(
        io.clone(),
        config.clone(),
        IndexMap::new(),
        false,
    )));

    FileDownloader::new(io, config, http_downloader, None, None, None, None)
}

#[test]
#[ignore]
fn test_get_file_name() {
    let package = get_package("dummy/pkg", "1.0.0");
    package.set_dist_url(Some("http://example.com/script.js".to_string()));

    let downloader = get_archive_downloader(Some("/vendor"));

    let first = downloader.__get_file_name(package.clone(), "/path");
    let re = regex::Regex::new(r"/vendor/composer/tmp-[a-z0-9]+\.js").unwrap();
    assert!(re.is_match(&first));
    assert_eq!(first, downloader.__get_file_name(package, "/path"));
}

#[test]
#[ignore]
fn test_process_url() {
    let downloader = get_archive_downloader(None);

    let expected = "https://github.com/composer/composer/zipball/master";
    let package = get_package("dummy/pkg", "1.0.0");
    let url = downloader.__process_url(package, expected).unwrap();

    assert_eq!(expected, url);
}

#[test]
#[ignore]
fn test_process_url2() {
    let downloader = get_archive_downloader(None);

    let expected = "https://github.com/composer/composer/archive/master.tar.gz";
    let package = get_package("dummy/pkg", "1.0.0");
    let url = downloader.__process_url(package, expected).unwrap();

    assert_eq!(expected, url);
}

#[test]
#[ignore]
fn test_process_url3() {
    let downloader = get_archive_downloader(None);

    let expected = "https://api.github.com/repos/composer/composer/zipball/master";
    let package = get_package("dummy/pkg", "1.0.0");
    let url = downloader.__process_url(package, expected).unwrap();

    assert_eq!(expected, url);
}

/// ref: ArchiveDownloaderTest::provideUrls
fn provide_urls() -> Vec<&'static str> {
    vec![
        "https://api.github.com/repos/composer/composer/zipball/master",
        "https://api.github.com/repos/composer/composer/tarball/master",
        "https://github.com/composer/composer/zipball/master",
        "https://www.github.com/composer/composer/tarball/master",
        "https://github.com/composer/composer/archive/master.zip",
        "https://github.com/composer/composer/archive/master.tar.gz",
    ]
}

#[test]
#[ignore]
fn test_process_url_rewrite_dist() {
    let downloader = get_archive_downloader(None);

    for url in provide_urls() {
        let r#type = if url.contains("tar") { "tar" } else { "zip" };
        let expected = format!(
            "https://api.github.com/repos/composer/composer/{}ball/ref",
            r#type
        );

        let package = get_package("dummy/pkg", "1.0.0");
        package.set_dist_reference(Some("ref".to_string()));
        let url = downloader.__process_url(package, url).unwrap();

        assert_eq!(expected, url);
    }
}

/// ref: ArchiveDownloaderTest::provideBitbucketUrls
fn provide_bitbucket_urls() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "https://bitbucket.org/davereid/drush-virtualhost/get/77ca490c26ac818e024d1138aa8bd3677d1ef21f",
            "zip",
        ),
        (
            "https://bitbucket.org/davereid/drush-virtualhost/get/master",
            "tar.gz",
        ),
        (
            "https://bitbucket.org/davereid/drush-virtualhost/get/v1.0",
            "tar.bz2",
        ),
    ]
}

#[test]
#[ignore]
fn test_process_url_rewrite_bitbucket_dist() {
    let downloader = get_archive_downloader(None);

    for (url, extension) in provide_bitbucket_urls() {
        let url = format!("{}.{}", url, extension);
        let expected = format!(
            "https://bitbucket.org/davereid/drush-virtualhost/get/ref.{}",
            extension
        );

        let package = get_package("dummy/pkg", "1.0.0");
        package.set_dist_reference(Some("ref".to_string()));
        let url = downloader.__process_url(package, &url).unwrap();

        assert_eq!(expected, url);
    }
}

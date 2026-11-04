//! ref: composer/tests/Composer/Test/Downloader/ArchiveDownloaderTest.php sha1:b491be023082501d01d424a28c7d8dd511691b57

// The PHP test builds an anonymous ArchiveDownloader subclass; getFileName/processUrl are
// inherited unchanged from FileDownloader, so the concrete FileDownloader is exercised here
// directly. The PHP mocks of IOInterface/Config/PackageInterface are replaced by a NullIO, a
// real Config merged with `vendor-dir`, and real CompletePackage instances with dist
// url/reference set.

use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::downloader::FileDownloader;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::HttpDownloader;
use shirabe_php_shim::{PhpMixed, bconcat, path_to_bytes};
use shirabe_semver::VersionParser;

/// ref: TestCase::getPackage (default class CompletePackage)
fn get_package(name: &BStr, version: &BStr) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(version, None).unwrap();
    CompletePackageHandle::new(bconcat!(name), norm_version, bconcat!(version)).into()
}

/// ref: ArchiveDownloaderTest::getArchiveDownloaderMock (the inherited getFileName/processUrl
/// live on FileDownloader, so the concrete downloader is built directly).
fn get_archive_downloader(vendor_dir: Option<&BStr>) -> FileDownloader {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));

    let mut config = Config::new(false, None);
    if let Some(vendor_dir) = vendor_dir {
        let mut config_options: IndexMap<BString, PhpMixed> = IndexMap::new();
        config_options.insert("vendor-dir".into(), PhpMixed::string(bconcat!(vendor_dir)));
        let mut merged: IndexMap<BString, PhpMixed> = IndexMap::new();
        merged.insert("config".into(), PhpMixed::Array(config_options));
        config.merge(&merged, "test");
    }
    let config = std::rc::Rc::new(std::cell::RefCell::new(config));

    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(
        HttpDownloader::new(io.clone(), config.clone(), IndexMap::new(), false).unwrap(),
    ));

    FileDownloader::new(io, config, http_downloader, None, None, None, None).unwrap()
}

#[test]
fn test_get_file_name() {
    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
    package.set_dist_url(Some(BString::from("http://example.com/script.js")));

    let downloader = get_archive_downloader(Some(BStr::new("/vendor")));

    let first = downloader
        .__get_file_name(package.clone(), std::path::Path::new("/path"))
        .unwrap();
    let re = regex::bytes::Regex::new(r"/vendor/composer/tmp-[a-z0-9]+\.js").unwrap();
    assert!(re.is_match(path_to_bytes(&first)));
    assert_eq!(
        first,
        downloader
            .__get_file_name(package, std::path::Path::new("/path"))
            .unwrap()
    );
}

#[test]
fn test_process_url() {
    let downloader = get_archive_downloader(None);

    let expected = "https://github.com/composer/composer/zipball/master";
    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
    let url = downloader.__process_url(package, expected).unwrap();

    assert_eq!(expected, url);
}

#[test]
fn test_process_url2() {
    let downloader = get_archive_downloader(None);

    let expected = "https://github.com/composer/composer/archive/master.tar.gz";
    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
    let url = downloader.__process_url(package, expected).unwrap();

    assert_eq!(expected, url);
}

#[test]
fn test_process_url3() {
    let downloader = get_archive_downloader(None);

    let expected = "https://api.github.com/repos/composer/composer/zipball/master";
    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
    let url = downloader.__process_url(package, expected).unwrap();

    assert_eq!(expected, url);
}

/// ref: ArchiveDownloaderTest::provideUrls
fn provide_urls() -> Vec<&'static BStr> {
    vec![
        BStr::new("https://api.github.com/repos/composer/composer/zipball/master"),
        BStr::new("https://api.github.com/repos/composer/composer/tarball/master"),
        BStr::new("https://github.com/composer/composer/zipball/master"),
        BStr::new("https://www.github.com/composer/composer/tarball/master"),
        BStr::new("https://github.com/composer/composer/archive/master.zip"),
        BStr::new("https://github.com/composer/composer/archive/master.tar.gz"),
    ]
}

#[test]
fn test_process_url_rewrite_dist() {
    let downloader = get_archive_downloader(None);

    for url in provide_urls() {
        let r#type = if url.contains_str("tar") {
            "tar"
        } else {
            "zip"
        };
        let expected = bconcat!(
            "https://api.github.com/repos/composer/composer/",
            r#type,
            "ball/ref"
        );

        let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
        package.set_dist_reference(Some(BString::from("ref")));
        let url = downloader.__process_url(package, url).unwrap();

        assert_eq!(expected, url);
    }
}

/// ref: ArchiveDownloaderTest::provideBitbucketUrls
fn provide_bitbucket_urls() -> Vec<(&'static BStr, &'static BStr)> {
    vec![
        (
            BStr::new(
                "https://bitbucket.org/davereid/drush-virtualhost/get/77ca490c26ac818e024d1138aa8bd3677d1ef21f",
            ),
            BStr::new("zip"),
        ),
        (
            BStr::new("https://bitbucket.org/davereid/drush-virtualhost/get/master"),
            BStr::new("tar.gz"),
        ),
        (
            BStr::new("https://bitbucket.org/davereid/drush-virtualhost/get/v1.0"),
            BStr::new("tar.bz2"),
        ),
    ]
}

#[test]
fn test_process_url_rewrite_bitbucket_dist() {
    let downloader = get_archive_downloader(None);

    for (url, extension) in provide_bitbucket_urls() {
        let url = bconcat!(url, ".", extension);
        let expected = bconcat!(
            "https://bitbucket.org/davereid/drush-virtualhost/get/ref.",
            extension
        );

        let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
        package.set_dist_reference(Some(BString::from("ref")));
        let url = downloader.__process_url(package, &url).unwrap();

        assert_eq!(expected, url);
    }
}

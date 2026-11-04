//! ref: composer/tests/Composer/Test/Package/Archiver/GitExcludeFilterTest.php sha1:934311419504913979476c79a7e8136e12be10af

use bstr::BStr;
use shirabe::package::archiver::git_exclude_filter::GitExcludeFilter;

#[test]
fn test_pattern_escape() {
    for (ignore, expected) in provide_patterns() {
        let filter = GitExcludeFilter::new("/");

        assert_eq!(expected, filter.parse_git_attributes_line(ignore));
    }
}

fn provide_patterns() -> Vec<(&'static BStr, Option<(bstr::BString, bool, bool)>)> {
    vec![
        (
            BStr::new("app/config/parameters.yml export-ignore"),
            Some((
                bstr::BString::from(r"{app/config/parameters\.yml(?:$|/)}"),
                false,
                false,
            )),
        ),
        (
            BStr::new("app/config/parameters.yml -export-ignore"),
            Some((
                bstr::BString::from(r"{app/config/parameters\.yml(?:$|/)}"),
                true,
                false,
            )),
        ),
    ]
}

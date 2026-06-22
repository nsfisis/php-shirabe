//! ref: composer/tests/Composer/Test/Package/Archiver/GitExcludeFilterTest.php

use shirabe::package::archiver::git_exclude_filter::GitExcludeFilter;

#[test]
#[ignore]
fn test_pattern_escape() {
    for (ignore, expected) in provide_patterns() {
        let filter = GitExcludeFilter::new("/".to_string());

        assert_eq!(expected, filter.parse_git_attributes_line(ignore));
    }
}

fn provide_patterns() -> Vec<(&'static str, Option<(String, bool, bool)>)> {
    vec![
        (
            "app/config/parameters.yml export-ignore",
            Some((
                r"{(?=[^\.])app/(?=[^\.])config/(?=[^\.])parameters\.yml(?=$|/)}".to_string(),
                false,
                false,
            )),
        ),
        (
            "app/config/parameters.yml -export-ignore",
            Some((
                r"{(?=[^\.])app/(?=[^\.])config/(?=[^\.])parameters\.yml(?=$|/)}".to_string(),
                true,
                false,
            )),
        ),
    ]
}

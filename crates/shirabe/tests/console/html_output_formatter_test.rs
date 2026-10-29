//! ref: composer/tests/Composer/Test/Console/HtmlOutputFormatterTest.php

use indexmap::IndexMap;
use shirabe::console::html_output_formatter::HtmlOutputFormatter;
use shirabe_symfony_console::formatter::{OutputFormatterStyle, OutputFormatterStyleInterface};

#[test]
fn test_formatting() {
    let mut styles: IndexMap<bstr::BString, Box<dyn OutputFormatterStyleInterface>> =
        IndexMap::new();
    styles.insert(
        "warning".into(),
        Box::new(OutputFormatterStyle::new(
            Some("black".into()),
            Some("yellow".into()),
            vec![],
        )),
    );

    let mut formatter = HtmlOutputFormatter::new(styles);

    assert_eq!(
        Some(
            "text <span style=\"color:green;\">green</span> <span style=\"color:yellow;\">yellow</span> <span style=\"color:black;background-color:yellow;\">black w/ yellow bg</span>"
                .into()
        ),
        formatter
            .format(Some(
                "text <info>green</info> <comment>yellow</comment> <warning>black w/ yellow bg</warning>"
                    .into()
            ))
            .unwrap()
    );
}

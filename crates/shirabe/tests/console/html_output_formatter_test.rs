//! ref: composer/tests/Composer/Test/Console/HtmlOutputFormatterTest.php

use indexmap::IndexMap;
use shirabe::console::html_output_formatter::HtmlOutputFormatter;
use shirabe_external_packages::symfony::console::formatter::{
    OutputFormatterStyle, OutputFormatterStyleInterface,
};

#[test]
fn test_formatting() {
    let mut styles: IndexMap<String, Box<dyn OutputFormatterStyleInterface>> = IndexMap::new();
    styles.insert(
        "warning".to_string(),
        Box::new(OutputFormatterStyle::new(
            Some("black"),
            Some("yellow"),
            vec![],
        )),
    );

    let mut formatter = HtmlOutputFormatter::new(styles);

    assert_eq!(
        Some(
            "text <span style=\"color:green;\">green</span> <span style=\"color:yellow;\">yellow</span> <span style=\"color:black;background-color:yellow;\">black w/ yellow bg</span>"
                .to_string()
        ),
        formatter
            .format(Some(
                "text <info>green</info> <comment>yellow</comment> <warning>black w/ yellow bg</warning>"
            ))
            .unwrap()
    );
}

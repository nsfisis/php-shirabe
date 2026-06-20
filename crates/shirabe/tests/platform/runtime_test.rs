//! ref: composer/tests/Composer/Test/Platform/RuntimeTest.php

use shirabe::platform::runtime::Runtime;

#[test]
#[ignore = "Runtime::parse_html_extension_info reaches a todo!() in the php-shim (html_entity_decode)"]
fn test_parse_extension_info() {
    for (html_input, expected_output) in provide_extension_infos() {
        assert_eq!(expected_output, Runtime::parse_html_extension_info(html_input));
    }
}

fn provide_extension_infos() -> Vec<(&'static str, &'static str)> {
    vec![(
        // 'pdo_sqlite'
        "<h2><a name=\"module_pdo_sqlite\" href=\"#module_pdo_sqlite\">pdo_sqlite</a></h2>
<table>
<tr><td class=\"e\">PDO Driver for SQLite 3.x </td><td class=\"v\">enabled </td></tr>
<tr><td class=\"e\">SQLite Library </td><td class=\"v\">3.40.1 </td></tr>
</table>",
        "pdo_sqlite

PDO Driver for SQLite 3.x => enabled
SQLite Library => 3.40.1",
    )]
}

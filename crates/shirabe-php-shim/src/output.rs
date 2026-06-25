// PHP output buffering captures everything the interpreter would echo to stdout. The shim has no
// general echo-to-buffer routing, and its only producer in Composer (`phpinfo`) depends on PHP
// runtime configuration that is itself unmodeled, so a buffer here would silently capture nothing.
pub fn ob_start() -> bool {
    todo!()
}

pub fn ob_get_clean() -> Option<String> {
    todo!()
}

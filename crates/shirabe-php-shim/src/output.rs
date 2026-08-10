// PHP output buffering captures everything the interpreter would echo to stdout. The shim has no
// general echo-to-buffer routing, so a buffer here would silently capture nothing.
pub fn ob_start() -> bool {
    todo!()
}

pub fn ob_get_clean() -> Option<String> {
    todo!()
}

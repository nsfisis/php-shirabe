//! ref: composer/tests/Composer/Test/Repository/Vcs/FossilDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::FossilDriver;

fn support_provider() -> Vec<(&'static str, bool)> {
    vec![
        ("http://fossil.kd2.org/kd2fw/", true),
        ("https://chiselapp.com/user/rkeene/repository/flint/index", true),
        ("ssh://fossil.kd2.org/kd2fw.fossil", true),
    ]
}

#[test]
fn test_support() {
    for (url, assertion) in support_provider() {
        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(Config::new(true, None)));
        let result = FossilDriver::supports(io, config, url, false).unwrap();
        assert_eq!(assertion, result);
    }
}

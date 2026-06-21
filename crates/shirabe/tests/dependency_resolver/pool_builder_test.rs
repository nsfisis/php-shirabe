//! ref: composer/tests/Composer/Test/DependencyResolver/PoolBuilderTest.php

// testPoolBuilder is a large fixture-driven case that loads packages from test inputs and
// builds a Pool; constraint parsing uses a look-around regex the regex crate cannot compile.
#[test]
#[ignore = "ArrayLoader::load (single-package, pub) and Pool::count are not exposed: the loadPackage closure calls $loader->load($data) per package and getPackageResultSet uses count($pool)"]
fn test_pool_builder() {
    todo!()
}

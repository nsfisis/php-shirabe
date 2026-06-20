//! ref: composer/tests/Composer/Test/DependencyResolver/TransactionTest.php

// Transaction::new sorts operations via shirabe_php_shim::uasort_map, which is todo!().
// The fixture also calls setRequires/setProvides on non-root packages, which the public
// handle API does not allow, so the scenario cannot be expressed faithfully yet.
#[test]
#[ignore = "Transaction::new reaches uasort_map (todo!()); fixture needs link setters on non-root packages"]
fn test_transaction_generation_and_sorting() {
    todo!()
}

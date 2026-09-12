//! v1 compatibility suite. Third parties can run `cargo test -p meno-store --test compat_v1`.

use meno_store::{BUNDLE_VERSION, CURRENT_SCHEMA_VERSION};

#[test]
fn v1_store_versions_are_frozen() {
    assert_eq!(BUNDLE_VERSION, 1);
    assert_eq!(CURRENT_SCHEMA_VERSION, 1);
}

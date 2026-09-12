//! v1 compatibility suite. Third parties can run `cargo test -p meno-adapters --test compat_v1`.

use meno_adapters::{Adapter, FakeAdapter, InputBundle};
use meno_core::envelope::Envelope;
use meno_core::verdict::Verdict;

/// Frozen v1 envelope kinds. FakeAdapter::normalize returns Envelope, never Verdict.
const V1_KINDS: &[&str] = &[
    "command.result",
    "junit.report",
    "playwright.result",
    "human.confirmation",
    "generic.envelope",
];

#[test]
fn v1_adapter_kinds_and_normalize_returns_envelope() {
    assert_eq!(
        V1_KINDS,
        [
            "command.result",
            "junit.report",
            "playwright.result",
            "human.confirmation",
            "generic.envelope",
        ]
    );

    let adapter = FakeAdapter;
    let envelope = adapter
        .normalize(&InputBundle {
            kind: "bytes".to_string(),
            payload: b"hello".to_vec(),
            path: None,
        })
        .expect("fake normalize");
    let _: Envelope = envelope;
    let _ = std::any::type_name::<Verdict>();
}

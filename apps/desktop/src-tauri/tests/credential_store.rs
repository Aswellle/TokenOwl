//! Cross-instance credential persistence.
//!
//! Account creation writes the secret through one `CredentialStore`; the
//! refresh path later builds a fresh store and resolves the same reference.
//! With keyring's mock backend (the silent default when no platform feature is
//! enabled) the secret lives inside the writing `Entry`, so this test fails —
//! exactly the regression that made every real account report `CredentialError`.
#![cfg(any(target_os = "windows", target_os = "macos"))]

use tokenowl_lib::credentials::CredentialStore;

#[test]
fn a_secret_written_by_one_store_is_readable_by_a_fresh_store() {
    let unique = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    let service = format!("com.tokenowl.test.{unique}");
    let reference = format!("account/test-{unique}");

    let writer = CredentialStore::new(service.clone());
    writer
        .set(&reference, "test-secret")
        .expect("store the secret");

    let reader = CredentialStore::new(service);
    let value = reader
        .resolve(&reference)
        .expect("a fresh store must resolve the secret");
    assert_eq!(value.as_str(), "test-secret");

    reader
        .delete(&reference)
        .expect("clean up the test credential");
}

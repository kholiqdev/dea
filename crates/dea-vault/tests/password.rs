use std::fs;

use dea_testkit::find_canary;
use dea_vault::{KdfParams, VaultError, VaultPassword};
use secrecy::SecretString;

#[test]
fn password_is_verified_against_its_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("password.phc");
    let password = VaultPassword::open(&path);
    assert!(!password.is_set());
    assert!(matches!(
        password.verify(&SecretString::from("anything")),
        Err(VaultError::NoPassword)
    ));

    password
        .set(
            &SecretString::from("correct horse"),
            KdfParams::insecure_for_tests(),
        )
        .unwrap();
    assert!(password.is_set());
    assert!(
        password
            .verify(&SecretString::from("correct horse"))
            .unwrap()
    );
    assert!(
        !password
            .verify(&SecretString::from("correct horsE"))
            .unwrap()
    );

    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("$argon2id$"), "{text}");
    // A second instance reads the same file.
    assert!(
        VaultPassword::open(&path)
            .verify(&SecretString::from("correct horse"))
            .unwrap()
    );
}

#[test]
fn short_password_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let password = VaultPassword::open(&dir.path().join("password.phc"));
    let result = password.set(
        &SecretString::from("1234567"),
        KdfParams::insecure_for_tests(),
    );
    assert!(matches!(
        result,
        Err(VaultError::PasswordTooShort { min: 8 })
    ));
    assert!(!password.is_set());
    // Eight characters, counted as characters rather than bytes.
    password
        .set(
            &SecretString::from("ééééàààà"),
            KdfParams::insecure_for_tests(),
        )
        .unwrap();
}

#[test]
fn hash_file_does_not_contain_the_password() {
    let dir = tempfile::tempdir().unwrap();
    let secret = dea_testkit::canary();
    VaultPassword::open(&dir.path().join("password.phc"))
        .set(
            &SecretString::from(secret.clone()),
            KdfParams::insecure_for_tests(),
        )
        .unwrap();
    assert!(find_canary(&[dir.path()], &secret).is_empty());
}

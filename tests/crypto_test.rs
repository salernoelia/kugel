use kugel::net::crypto::{CredentialsStore, E2eeCipher, UserCredentials};

#[test]
fn test_e2ee_aes_gcm_encryption_and_decryption() {
    let room_key = E2eeCipher::derive_room_key("super_secret_room_key");
    let cipher = E2eeCipher::new_from_key(&room_key);

    let payload = b"Sensitive collaborative vector delta data";
    let seq_no = 500;

    let frame = cipher.encrypt(payload, seq_no).expect("Encryption failed");
    assert_eq!(frame.seq_no, 500);
    assert_ne!(frame.ciphertext, payload);

    let decrypted = cipher.decrypt(&frame).expect("Decryption failed");
    assert_eq!(decrypted, payload);
}

#[test]
fn test_credentials_saved_separately_from_pointer() {
    let creds = UserCredentials {
        user_id: "user_test_sec".to_string(),
        display_name: "SecUser".to_string(),
        auth_token: "jwt_secret_never_in_kugelsh".to_string(),
        color: [255, 128, 0, 255],
    };

    assert!(CredentialsStore::save(&creds).is_ok());
    let loaded = CredentialsStore::load();
    assert_eq!(loaded.auth_token, creds.auth_token);
}

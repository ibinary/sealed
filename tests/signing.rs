use sealed::signing::SealedKeyPair;

#[test]
fn sign_and_verify() {
    let kp = SealedKeyPair::generate();
    let envelope = kp.sign("test payload");
    assert!(envelope.verify().is_ok());
}

#[test]
fn tampered_payload_fails() {
    let kp = SealedKeyPair::generate();
    let mut envelope = kp.sign("test payload");
    envelope.payload = "tampered".to_string();
    assert!(envelope.verify().is_err());
}

#[test]
fn wrong_key_fails() {
    let kp1 = SealedKeyPair::generate();
    let kp2 = SealedKeyPair::generate();
    let mut envelope = kp1.sign("test payload");
    envelope.public_key = kp2.public_key_base64();
    assert!(envelope.verify().is_err());
}

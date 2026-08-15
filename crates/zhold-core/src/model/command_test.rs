use super::{CargoCommandClass, CommandDescriptor};

#[test]
fn command_descriptors_classify_without_retaining_arguments() {
    let secret = "registries.private.token='extremely-secret-token'";
    let arguments = vec![
        "+nightly".to_owned(),
        "--config".to_owned(),
        secret.to_owned(),
        "test".to_owned(),
    ];

    let descriptor = CommandDescriptor::from_arguments(&arguments, &[7; 32]);

    assert_eq!(descriptor.command_class, CargoCommandClass::Test);
    assert!(!format!("{descriptor:?}").contains(secret));
    assert_eq!(descriptor.arguments_fingerprint.len(), 32);
}

#[test]
fn command_fingerprints_are_scoped_to_a_private_store_key() {
    let arguments = vec!["test".to_owned(), "--token=guessable".to_owned()];

    let first = CommandDescriptor::from_arguments(&arguments, &[1; 32]);
    let repeated = CommandDescriptor::from_arguments(&arguments, &[1; 32]);
    let other_store = CommandDescriptor::from_arguments(&arguments, &[2; 32]);

    assert_eq!(first.arguments_fingerprint, repeated.arguments_fingerprint);
    assert_ne!(
        first.arguments_fingerprint,
        other_store.arguments_fingerprint
    );
}

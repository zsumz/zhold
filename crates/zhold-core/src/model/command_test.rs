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

    let descriptor = CommandDescriptor::from_arguments(&arguments);

    assert_eq!(descriptor.command_class, CargoCommandClass::Test);
    assert!(!format!("{descriptor:?}").contains(secret));
    assert_eq!(descriptor.arguments_fingerprint.len(), 32);
}

pub(super) fn digest(namespace: &str, parts: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(namespace.as_bytes());
    hasher.update(&[0]);

    for part in parts {
        let length = u64::try_from(part.len()).unwrap_or(u64::MAX);
        hasher.update(&length.to_le_bytes());
        hasher.update(part.as_bytes());
    }

    hasher.finalize().to_hex()[..32].to_owned()
}

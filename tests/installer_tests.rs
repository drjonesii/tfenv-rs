#[test]
fn test_asset_name() {
    use tfenv_rs::installer::asset_name;
    let s = asset_name("terraform", "1.0.0");
    assert!(s.starts_with("terraform_1.0.0_"));
}

#[test]
fn test_map_arch_and_os() {
    use tfenv_rs::installer::{map_arch, map_os};
    let _ = map_os();
    let _ = map_arch();
    // At minimum they should return non-empty strings
    assert!(!map_os().is_empty());
    assert!(!map_arch().is_empty());
}

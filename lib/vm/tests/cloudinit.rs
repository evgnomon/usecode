use std::path::PathBuf;

/// Generates a real ISO through libisofs to check the C glue and its linkage.
#[test]
fn creates_a_cidata_iso_from_a_template() {
    let dir = PathBuf::from(format!("/tmp/vm-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let template = dir.join("cloud-init-user-data.yaml");
    std::fs::write(&template, "#cloud-config\nusers:\n  - name: vm\n").unwrap();

    let iso = dir.join("out.iso");
    vm::cloudinit::create_cloud_init_iso("vm-test-vm", &template, &iso).unwrap();

    let bytes = std::fs::read(&iso).unwrap();
    assert!(!bytes.is_empty());
    // The volume id lands in the primary volume descriptor.
    assert!(
        bytes.windows(6).any(|w| w == b"CIDATA"),
        "ISO should carry the CIDATA volume id"
    );
    // The temporary user-data/meta-data files are cleaned up.
    assert!(!PathBuf::from("/tmp/vm-test-vm-user-data").exists());

    std::fs::remove_dir_all(&dir).unwrap();
}

// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Cloud-init NoCloud ISO generation (backed by libisofs through src/iso.c).

use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::wyhash;

unsafe extern "C" {
    fn vm_geniso(
        output: *const c_char,
        user_data: *const c_char,
        meta_data: *const c_char,
    ) -> c_int;
}

/// Writes user-data/meta-data for `domain_name` and packs them into a `cidata` ISO.
pub fn create_cloud_init_iso(
    domain_name: &str,
    template_path: impl AsRef<Path>,
    output_iso_path: impl AsRef<Path>,
) -> Result<()> {
    let output_iso_path = output_iso_path.as_ref();

    let template_content = std::fs::read_to_string(template_path)?;

    // Combine template with bootcmd to regenerate machine-id.
    let user_data_path = PathBuf::from(format!("/tmp/{domain_name}-user-data"));
    let user_data_content = format!(
        "{template_content}\nbootcmd:\n  - rm -f /etc/machine-id\n  - systemd-machine-id-setup\n"
    );
    std::fs::write(&user_data_path, user_data_content)?;

    // Generate a unique instance-id based on the domain name hash.
    let hash = wyhash::hash(0, domain_name.as_bytes());
    let meta_data_path = PathBuf::from(format!("/tmp/{domain_name}-meta-data"));
    let meta_content =
        format!("instance-id: {domain_name}-{hash:x}\nlocal-hostname: {domain_name}\n");
    std::fs::write(&meta_data_path, meta_content)?;

    // Delete existing ISO if it exists.
    if let Err(err) = std::fs::remove_file(output_iso_path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        warn!("Could not delete old ISO: {err}");
    }

    let result = create_iso(output_iso_path, &user_data_path, &meta_data_path);

    // Clean up temporary files.
    for path in [&user_data_path, &meta_data_path] {
        if let Err(err) = std::fs::remove_file(path) {
            warn!("Could not delete temp file {}: {err}", path.display());
        }
    }

    result
}

fn create_iso(output: &Path, user_data: &Path, meta_data: &Path) -> Result<()> {
    let to_cstring = |path: &Path| -> Result<CString> {
        CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| Error::vm(format!("path contains a NUL byte: {}", path.display())))
    };

    let c_output = to_cstring(output)?;
    let c_user_data = to_cstring(user_data)?;
    let c_meta_data = to_cstring(meta_data)?;

    let rc = unsafe {
        vm_geniso(
            c_output.as_ptr(),
            c_user_data.as_ptr(),
            c_meta_data.as_ptr(),
        )
    };
    if rc != 0 {
        error!("Failed to create cloud-init ISO: exit code {rc}");
        return Err(Error::Iso(rc));
    }

    Ok(())
}

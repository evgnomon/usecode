//! VM lifecycle: create, fork, start/stop, inspect, snapshot and mount.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::cloudinit;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::libvirt::{Connection, Domain};
use crate::network;
use crate::ssh_conf::{self, SshConfig};

/// A host directory shared into the VM over virtiofs.
#[derive(Debug, Clone)]
pub struct MountSpec {
    pub host_path: String,
    pub tag: String,
}

#[derive(Debug, Clone)]
pub struct VmSpecs {
    /// Memory in KiB.
    pub memory: u64,
    pub vcpus: u32,
    pub machine: String,
    /// Disk size in bytes.
    pub disk_size: u64,
    pub image_path: Option<String>,
    pub start: bool,
    pub wait_for_ip: bool,
    pub mounts: Vec<MountSpec>,
}

impl Default for VmSpecs {
    fn default() -> Self {
        VmSpecs {
            memory: 1024 * 1024,
            vcpus: 2,
            machine: "pc-q35-10.0".to_string(),
            disk_size: 10 * 1024 * 1024 * 1024,
            image_path: None,
            start: true,
            wait_for_ip: true,
            mounts: Vec::new(),
        }
    }
}

/// Suffix of the overlay a VM runs on once it has been used as a fork source.
const FORK_SUFFIX: &str = "-fork.qcow2";

pub fn create_vm(
    conn: &Connection,
    cfg: &Config,
    domain_name: &str,
    specs: &VmSpecs,
) -> Result<()> {
    if domain_name.is_empty() {
        return Err(Error::vm("invalid domain name"));
    }

    if conn.lookup_domain(domain_name).is_ok() {
        return Err(Error::vm(format!("VM '{domain_name}' already exists")));
    }

    let src_image = match &specs.image_path {
        Some(path) => PathBuf::from(path),
        None => Path::new(&cfg.base_image_path).join(&cfg.image_name),
    };
    let dst_image = format!("{}/{domain_name}.qcow2", cfg.vm_storage_path);

    // 1. Ensure destination directory exists
    if let Err(err) = std::fs::create_dir_all(&cfg.vm_storage_path) {
        error!(
            "Failed to create storage directory {}: {err}",
            cfg.vm_storage_path
        );
        return Err(err.into());
    }

    // 2. Copy the source image
    info!("Copying image {} -> {dst_image}", src_image.display());
    std::fs::copy(&src_image, &dst_image)?;

    // 3. Fix ownership and permissions
    let mode = std::fs::metadata(&dst_image)?.permissions().mode();
    if mode & 0o666 != 0o660 {
        std::fs::set_permissions(&dst_image, std::fs::Permissions::from_mode(0o666))?;
    }

    // 4. Resize disk image
    info!("Resizing disk image to {} bytes", specs.disk_size);
    if let Err(err) = conn.resize_volume(&dst_image, specs.disk_size) {
        error!("Failed to resize disk image: {err}");
        if let Err(del_err) = std::fs::remove_file(&dst_image) {
            warn!("Failed to clean up copied disk image {dst_image}: {del_err}");
        }
        return Err(Error::vm("failed to resize disk image"));
    }

    // 5. Create cloud-init ISO
    let cloud_init_template =
        Path::new(&cfg.cloud_init_template_path).join("cloud-init-user-data.yaml");

    // Ensure cloud-init user-data template exists; create from config if missing.
    if let Err(err) = ensure_cloud_init_template(cfg, &cloud_init_template) {
        warn!("Could not ensure cloud-init template: {err}");
    }

    let cloud_init_iso = format!("{}/{domain_name}-cloud-init.iso", cfg.vm_storage_path);
    info!("Creating cloud-init ISO at {cloud_init_iso}");
    cloudinit::create_cloud_init_iso(domain_name, &cloud_init_template, &cloud_init_iso)?;

    // 6. Generate MAC address and domain XML
    let mac_addr = network::generate_mac_address(domain_name);
    let xml = generate_domain_xml(domain_name, &dst_image, &cloud_init_iso, &mac_addr, specs);

    // 7. Define domain
    info!("Defining domain '{domain_name}'");
    let dom = Domain::define_xml(conn, &xml)?;

    // 8. Start domain if requested
    if !specs.start {
        info!("Domain '{domain_name}' created but not started");
        return Ok(());
    }

    info!("Starting domain '{domain_name}'");
    dom.create()?;
    info!("Domain '{domain_name}' created and started");

    // 9. Wait for IP and register an SSH host entry
    if specs.wait_for_ip {
        register_host(&dom, cfg, domain_name, &mac_addr);
    }

    Ok(())
}

/// Waits for the VM's IP and writes its ssh_config.d entry; failures are non-fatal.
fn register_host(dom: &Domain, cfg: &Config, domain_name: &str, mac_addr: &str) {
    let ip = match network::get_ip_address(dom, mac_addr, cfg.max_retries) {
        Ok(ip) => ip,
        Err(err) => {
            warn!("Could not retrieve IP address: {err}");
            return;
        }
    };
    info!("VM IP address: {ip}");

    let conf = SshConfig {
        host: domain_name,
        hostname: &ip,
        user: &cfg.username,
        port: 22,
        identity_file: &cfg.identity_file,
    };
    if let Err(err) = ssh_conf::create_ssh_host_config(&conf) {
        warn!("Could not create SSH config: {err}");
    }
}

pub fn delete_vm(conn: &Connection, cfg: &Config, domain_name: &str, force: bool) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;

    // Stop if running
    if dom.is_active() {
        if !force {
            error!("Domain '{domain_name}' is running. Use --force to stop and delete.");
            return Err(Error::vm(format!("domain '{domain_name}' is running")));
        }
        info!("Force stopping domain '{domain_name}'");
        dom.destroy()?;
    }

    // Determine the actual disk path before undefining so we can delete it.
    let actual_disk_path = dom.xml().ok().and_then(|xml| extract_vda_disk_path(&xml));

    info!("Undefining domain '{domain_name}'");
    dom.undefine()?;

    // Delete disk image.
    // If the VM was a fork source its active disk is the overlay (<name>-fork.qcow2).
    // Delete only that overlay and leave the base image (<name>.qcow2) intact so
    // any derived fork VMs continue to function.
    match actual_disk_path {
        Some(disk_path) => {
            if disk_path.ends_with(FORK_SUFFIX) {
                info!("Removing fork overlay '{disk_path}' (base image preserved)");
            }
            remove_file_if_present(&disk_path, "Could not delete disk image");
        }
        None => {
            // Fallback: try the conventional path
            let fallback = format!("{}/{domain_name}.qcow2", cfg.vm_storage_path);
            remove_file_if_present(&fallback, "Could not delete disk image");
        }
    }

    if let Err(err) = ssh_conf::remove_ssh_host_config(domain_name) {
        warn!("Could not remove SSH config: {err}");
    }

    info!("Domain '{domain_name}' deleted");
    Ok(())
}

fn remove_file_if_present(path: &str, context: &str) {
    if let Err(err) = std::fs::remove_file(path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        warn!("{context}: {err}");
    }
}

pub fn start_vm(conn: &Connection, cfg: &Config, domain_name: &str) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;

    if dom.is_active() {
        info!("Domain '{domain_name}' is already running");
        return Ok(());
    }

    // Regenerate cloud-init ISO if the referenced path is missing.
    if let Ok(xml) = dom.xml()
        && let Some(iso_path) = extract_cdrom_path(&xml)
        && !Path::new(&iso_path).exists()
    {
        info!("Cloud-init ISO missing at '{iso_path}', regenerating");
        let template = Path::new(&cfg.cloud_init_template_path).join("cloud-init-user-data.yaml");
        if let Err(err) = ensure_cloud_init_template(cfg, &template) {
            warn!("Could not ensure cloud-init template: {err}");
        }
        if let Err(err) = cloudinit::create_cloud_init_iso(domain_name, &template, &iso_path) {
            warn!("Could not regenerate cloud-init ISO: {err}");
        }
    }

    info!("Starting domain '{domain_name}'");
    dom.create()?;
    info!("Domain '{domain_name}' started");
    Ok(())
}

pub fn stop_vm(conn: &Connection, domain_name: &str, force: bool) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;

    if !dom.is_active() {
        info!("Domain '{domain_name}' is not running");
        return Ok(());
    }

    if force {
        info!("Force stopping domain '{domain_name}'");
        dom.destroy()?;
    } else {
        // Graceful shutdown via ACPI
        info!("Gracefully stopping domain '{domain_name}'");
        dom.shutdown()?;
    }

    info!("Domain '{domain_name}' stopped");
    Ok(())
}

pub fn restart_vm(conn: &Connection, cfg: &Config, domain_name: &str, force: bool) -> Result<()> {
    {
        let dom = conn.lookup_domain(domain_name)?;

        if dom.is_active() {
            if force {
                info!("Force stopping domain '{domain_name}'");
            } else {
                info!("Stopping domain '{domain_name}'");
            }
            dom.destroy()?;
            info!("Domain '{domain_name}' stopped");
        }
    }

    start_vm(conn, cfg, domain_name)
}

pub fn get_vm_ip(conn: &Connection, cfg: &Config, domain_name: &str) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;

    if !dom.is_active() {
        error!("Domain '{domain_name}' is not running");
        return Err(Error::vm(format!("domain '{domain_name}' is not running")));
    }

    let mac_addr = network::generate_mac_address(domain_name);
    match network::get_ip_address(&dom, &mac_addr, cfg.max_retries) {
        Ok(ip) => info!("{domain_name}: {ip}"),
        Err(err) => warn!("Could not retrieve IP address: {err}"),
    }

    Ok(())
}

pub fn list_vms(conn: &Connection, all: bool) -> Result<()> {
    let domains = conn.list_domains(all)?;

    if domains.is_empty() {
        info!("No {}domains", if all { "" } else { "running " });
        return Ok(());
    }

    info!("{}domains:", if all { "All " } else { "Running " });
    for name in domains {
        info!("  {name}");
    }

    Ok(())
}

pub fn show_vm_info(conn: &Connection, domain_name: &str) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;

    info!("Domain: {}", dom.name());
    info!("  Active: {}", if dom.is_active() { "yes" } else { "no" });

    Ok(())
}

pub fn inspect_vm(conn: &Connection, domain_name: &str) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;

    info!("Domain: {domain_name}");
    info!(
        "  State:  {}",
        if dom.is_active() {
            "running"
        } else {
            "stopped"
        }
    );

    match dom.info() {
        Ok(info) => {
            let ram_mib = info.memory_kib / 1024;
            if ram_mib >= 1024 {
                info!("  RAM:    {} GiB", ram_mib / 1024);
            } else {
                info!("  RAM:    {ram_mib} MiB");
            }
            info!("  vCPUs:  {}", info.vcpus);
        }
        Err(_) => info!("  RAM:    (unavailable)"),
    }

    if let Ok(xml) = dom.xml() {
        let capacity = extract_vda_disk_path(&xml)
            .and_then(|disk_path| conn.volume_size_by_path(&disk_path).ok());
        match capacity {
            Some(cap_bytes) => {
                let cap_gib = cap_bytes / (1024 * 1024 * 1024);
                if cap_gib > 0 {
                    info!("  Disk:   {cap_gib} GiB");
                } else {
                    info!("  Disk:   {} MiB", cap_bytes / (1024 * 1024));
                }
            }
            None => info!("  Disk:   (unavailable)"),
        }
    }

    if dom.is_active() {
        let mac_addr = network::generate_mac_address(domain_name);
        match network::get_ip_address(&dom, &mac_addr, 1) {
            Ok(ip) => info!("  IP:     {ip}"),
            Err(_) => info!("  IP:     (not available)"),
        }
    } else {
        info!("  IP:     (VM is stopped)");
    }

    Ok(())
}

pub fn config_vm(conn: &Connection, domain_name: &str, memory_kib: u64) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;

    let ram_mib = memory_kib / 1024;
    if ram_mib >= 1024 {
        info!(
            "Setting memory of '{domain_name}' to {} GiB",
            ram_mib / 1024
        );
    } else {
        info!("Setting memory of '{domain_name}' to {ram_mib} MiB");
    }

    let live_applied = dom.set_memory(memory_kib)?;

    if dom.is_active() && !live_applied {
        info!("Config updated. Restart the VM for the new memory to take effect.");
    } else {
        info!("Memory updated successfully.");
    }

    Ok(())
}

pub fn create_snapshot(conn: &Connection, domain_name: &str, snapshot_name: &str) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;

    info!("Creating snapshot '{snapshot_name}' for domain '{domain_name}'");
    let _snap = dom.create_snapshot(snapshot_name)?;

    info!("Snapshot '{snapshot_name}' created");
    Ok(())
}

pub fn delete_snapshot(conn: &Connection, domain_name: &str, snapshot_name: &str) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;

    info!("Deleting snapshot '{snapshot_name}' from domain '{domain_name}'");
    dom.delete_snapshot(snapshot_name)?;

    info!("Snapshot '{snapshot_name}' deleted");
    Ok(())
}

pub fn restore_snapshot(conn: &Connection, domain_name: &str, snapshot_name: &str) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;

    info!("Reverting domain '{domain_name}' to snapshot '{snapshot_name}'");
    dom.revert_to_snapshot(snapshot_name)?;

    info!("Domain '{domain_name}' reverted to snapshot '{snapshot_name}'");
    Ok(())
}

pub fn list_snapshots(conn: &Connection, domain_name: &str) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;
    let snapshots = dom.list_snapshots()?;

    if snapshots.is_empty() {
        info!("No snapshots for domain '{domain_name}'");
        return Ok(());
    }

    info!("Snapshots for domain '{domain_name}':");
    for name in snapshots {
        info!("  {name}");
    }

    Ok(())
}

pub fn fork_vm(
    conn: &Connection,
    cfg: &Config,
    source_name: &str,
    dest_name: &str,
    specs: &VmSpecs,
) -> Result<()> {
    if dest_name.is_empty() {
        return Err(Error::vm("invalid domain name"));
    }

    // Verify source VM exists
    let src_dom = conn.lookup_domain(source_name)?;

    if src_dom.is_active() {
        warn!("Source domain '{source_name}' is running; forked disk may not be crash-consistent");
    }

    // Destination must not already exist
    if conn.lookup_domain(dest_name).is_ok() {
        return Err(Error::vm(format!("VM '{dest_name}' already exists")));
    }

    let dst_disk = format!("{}/{dest_name}.qcow2", cfg.vm_storage_path);

    // Read source persistent XML to find the original disk path (the fork point).
    let src_xml = src_dom.xml()?;
    let src_disk = extract_vda_disk_path(&src_xml)
        .ok_or_else(|| Error::vm(format!("could not find disk path for '{source_name}'")))?;

    // If the source VM is already running on a fork overlay (<name>-fork.qcow2), resolve
    // back to the true base image (<name>.qcow2) so all forks share the same backing.
    let already_forked = src_disk.ends_with(FORK_SUFFIX);
    let base_disk = if already_forked {
        format!("{}.qcow2", &src_disk[..src_disk.len() - FORK_SUFFIX.len()])
    } else {
        src_disk.clone()
    };

    if src_dom.is_active() && !already_forked {
        // Source is running and has not been forked yet: pivot it to a source-owned
        // continuation overlay so its QEMU process releases src_disk as the active
        // (write-locked) file.  After the snapshot:
        //   <name>-fork.qcow2  <- source's new active disk (write-locked by its QEMU)
        //   <name>.qcow2       <- frozen fork point (read-only backing for all forks)
        let src_cont = format!("{}/{source_name}{FORK_SUFFIX}", cfg.vm_storage_path);

        info!("Creating external disk snapshot for '{source_name}' at {src_cont}");
        src_dom.create_external_disk_snapshot("vda", &src_cont)?;

        // Update source's persistent config to reflect its new active disk path.
        let updated_src_xml = src_xml.replace(&src_disk, &src_cont);
        if let Err(err) = Domain::define_xml(conn, &updated_src_xml) {
            warn!("Could not update source VM config after snapshot: {err}");
        }
    } else if already_forked {
        info!(
            "Source '{source_name}' already has a fork overlay; using base image '{base_disk}' for new VM"
        );
    }

    // Create the dest disk as a fresh qcow2 overlay backed by base_disk.
    // No running QEMU holds a write lock on dst_disk, so the forked VM can start cleanly.
    let dst_vol_name = Path::new(&dst_disk)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dst_disk.clone());
    info!("Creating fork disk '{dst_disk}' with backing '{base_disk}'");
    conn.create_volume_with_backing(&dst_vol_name, &base_disk)?;

    // Cloud-init ISO for the new VM name
    let cloud_init_template =
        Path::new(&cfg.cloud_init_template_path).join("cloud-init-user-data.yaml");
    if let Err(err) = ensure_cloud_init_template(cfg, &cloud_init_template) {
        warn!("Could not ensure cloud-init template: {err}");
    }

    let cloud_init_iso = format!("{}/{dest_name}-cloud-init.iso", cfg.vm_storage_path);
    info!("Creating cloud-init ISO at {cloud_init_iso}");
    cloudinit::create_cloud_init_iso(dest_name, &cloud_init_template, &cloud_init_iso)?;

    let mac_addr = network::generate_mac_address(dest_name);
    let xml = generate_domain_xml(dest_name, &dst_disk, &cloud_init_iso, &mac_addr, specs);

    info!("Defining domain '{dest_name}'");
    let dom = Domain::define_xml(conn, &xml)?;

    if !specs.start {
        info!("Domain '{dest_name}' forked but not started");
        return Ok(());
    }

    info!("Starting domain '{dest_name}'");
    dom.create()?;
    info!("Domain '{dest_name}' forked and started");

    if specs.wait_for_ip {
        register_host(&dom, cfg, dest_name, &mac_addr);
    }

    Ok(())
}

pub fn mount_vm(conn: &Connection, domain_name: &str, mount: &MountSpec) -> Result<()> {
    let dom = conn.lookup_domain(domain_name)?;

    // virtiofs requires shared memory backing; patch the persistent XML if missing.
    let domain_xml = dom.xml()?;
    if !domain_xml.contains("access mode='shared'") {
        let insert_after = "</currentMemory>";
        let patch = "\n  <memoryBacking>\n    <source type='memfd'/>\n    <access mode='shared'/>\n  </memoryBacking>";
        if let Some(pos) = domain_xml.find(insert_after) {
            let split = pos + insert_after.len();
            let patched = format!("{}{patch}{}", &domain_xml[..split], &domain_xml[split..]);
            Domain::define_xml(conn, &patched)?;
            info!("Updated domain '{domain_name}' config to enable virtiofs shared memory.");
        }
        if dom.is_active() {
            error!(
                "Domain '{domain_name}' must be restarted for shared memory to take effect. Run: vm stop {domain_name} && vm start {domain_name}"
            );
            return Err(Error::vm(format!(
                "domain '{domain_name}' must be restarted"
            )));
        }
    }

    let fs_xml = format!(
        "<filesystem type='mount' accessmode='passthrough'>\n  <driver type='virtiofs'/>\n  <source dir='{}'/>\n  <target dir='{}'/>\n</filesystem>",
        mount.host_path, mount.tag,
    );
    dom.attach_device(&fs_xml)?;

    if dom.is_active() {
        info!(
            "Mounted '{}' as tag '{}' in running domain '{domain_name}'",
            mount.host_path, mount.tag
        );
        info!(
            "Inside the VM, run: sudo mkdir -p /mnt/{tag} && sudo mount -t virtiofs {tag} /mnt/{tag}",
            tag = mount.tag
        );
    } else {
        info!(
            "Mount '{}' -> tag '{}' added to domain '{domain_name}'",
            mount.host_path, mount.tag
        );
    }

    Ok(())
}

/// Writes a cloud-init user-data template from the configured user and SSH key
/// when none exists yet.
fn ensure_cloud_init_template(cfg: &Config, template_path: &Path) -> Result<()> {
    if template_path.exists() {
        return Ok(());
    }

    if cfg.ssh_key.is_empty() {
        error!("Cloud-init template missing and no ssh_key configured in /etc/vm/config.yaml");
        return Err(Error::vm("missing ssh_key in config"));
    }

    info!(
        "Creating cloud-init template at {}",
        template_path.display()
    );

    let content = format!(
        "#cloud-config\nusers:\n  - name: {}\n    sudo: ALL=(ALL) NOPASSWD:ALL\n    shell: /bin/bash\n    ssh_authorized_keys:\n      - {}\n",
        cfg.username, cfg.ssh_key,
    );

    if let Some(parent) = template_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(template_path, content)?;
    Ok(())
}

/// Extracts the source file path for the primary disk (device='disk') from domain XML.
fn extract_vda_disk_path(xml: &str) -> Option<String> {
    extract_source_file(xml, "device='disk'")
}

/// Extracts the source file path of the cloud-init CD-ROM from domain XML.
fn extract_cdrom_path(xml: &str) -> Option<String> {
    extract_source_file(xml, "device='cdrom'")
}

/// Returns the first `<source file='...'/>` value that follows `marker`.
fn extract_source_file(xml: &str, marker: &str) -> Option<String> {
    let pos = xml.find(marker)?;
    let after = &xml[pos..];

    for (needle, quote) in [("<source file='", '\''), ("<source file=\"", '"')] {
        if let Some(idx) = after.find(needle) {
            let start = idx + needle.len();
            if let Some(end) = after[start..].find(quote) {
                return Some(after[start..start + end].to_string());
            }
        }
    }

    None
}

fn generate_filesystems_xml(mounts: &[MountSpec]) -> String {
    let mut out = String::new();
    for mount in mounts {
        out.push_str(&format!(
            "    <filesystem type='mount' accessmode='passthrough'>\n      <driver type='virtiofs'/>\n      <source dir='{}'/>\n      <target dir='{}'/>\n    </filesystem>\n",
            mount.host_path, mount.tag,
        ));
    }
    out
}

fn generate_domain_xml(
    domain_name: &str,
    disk_path: &str,
    iso_path: &str,
    mac_addr: &str,
    specs: &VmSpecs,
) -> String {
    let filesystems = generate_filesystems_xml(&specs.mounts);
    let memory = specs.memory;
    let vcpus = specs.vcpus;
    let machine = &specs.machine;

    format!(
        r#"<domain type='kvm'>
  <name>{domain_name}</name>
  <metadata>
    <libosinfo:libosinfo xmlns:libosinfo="http://libosinfo.org/xmlns/libvirt/domain/1.0">
      <libosinfo:os id="http://debian.org/debian/12"/>
    </libosinfo:libosinfo>
  </metadata>
  <memory unit='KiB'>{memory}</memory>
  <currentMemory unit='KiB'>{memory}</currentMemory>
  <memoryBacking>
    <source type='memfd'/>
    <access mode='shared'/>
  </memoryBacking>
  <vcpu placement='static'>{vcpus}</vcpu>
  <resource>
    <partition>/machine</partition>
  </resource>
  <sysinfo type='smbios'>
    <system>
      <entry name='serial'>ds=nocloud</entry>
    </system>
  </sysinfo>
  <os>
    <type arch='x86_64' machine='{machine}'>hvm</type>
    <boot dev='hd'/>
    <smbios mode='sysinfo'/>
  </os>
  <features>
    <acpi/>
    <apic/>
    <vmport state='off'/>
  </features>
  <cpu mode='host-passthrough' check='none' migratable='on'/>
  <clock offset='utc'>
    <timer name='rtc' tickpolicy='catchup'/>
    <timer name='pit' tickpolicy='delay'/>
    <timer name='hpet' present='no'/>
  </clock>
  <on_poweroff>destroy</on_poweroff>
  <on_reboot>destroy</on_reboot>
  <on_crash>destroy</on_crash>
  <pm>
    <suspend-to-mem enabled='no'/>
    <suspend-to-disk enabled='no'/>
  </pm>
  <devices>
    <emulator>/usr/bin/qemu-system-x86_64</emulator>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2'/>
      <source file='{disk_path}'/>
      <target dev='vda' bus='virtio'/>
      <address type='pci' domain='0x0000' bus='0x04' slot='0x00' function='0x0'/>
    </disk>
    <disk type='file' device='cdrom'>
      <driver name='qemu' type='raw'/>
      <source file='{iso_path}'/>
      <backingStore/>
      <target dev='sda' bus='sata'/>
      <readonly/>
      <address type='drive' controller='0' bus='0' target='0' unit='0'/>
    </disk>
    <controller type='usb' index='0' model='qemu-xhci' ports='15'>
      <address type='pci' domain='0x0000' bus='0x02' slot='0x00' function='0x0'/>
    </controller>
    <controller type='pci' index='0' model='pcie-root'/>
    <controller type='pci' index='1' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='1' port='0x10'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x02' function='0x0' multifunction='on'/>
    </controller>
    <controller type='pci' index='2' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='2' port='0x11'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x02' function='0x1'/>
    </controller>
    <controller type='pci' index='3' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='3' port='0x12'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x02' function='0x2'/>
    </controller>
    <controller type='pci' index='4' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='4' port='0x13'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x02' function='0x3'/>
    </controller>
    <controller type='pci' index='5' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='5' port='0x14'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x02' function='0x4'/>
    </controller>
    <controller type='pci' index='6' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='6' port='0x15'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x02' function='0x5'/>
    </controller>
    <controller type='pci' index='7' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='7' port='0x16'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x02' function='0x6'/>
    </controller>
    <controller type='pci' index='8' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='8' port='0x17'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x02' function='0x7'/>
    </controller>
    <controller type='pci' index='9' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='9' port='0x18'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x03' function='0x0' multifunction='on'/>
    </controller>
    <controller type='pci' index='10' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='10' port='0x19'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x03' function='0x1'/>
    </controller>
    <controller type='pci' index='11' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='11' port='0x1a'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x03' function='0x2'/>
    </controller>
    <controller type='pci' index='12' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='12' port='0x1b'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x03' function='0x3'/>
    </controller>
    <controller type='pci' index='13' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='13' port='0x1c'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x03' function='0x4'/>
    </controller>
    <controller type='pci' index='14' model='pcie-root-port'>
      <model name='pcie-root-port'/>
      <target chassis='14' port='0x1d'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x03' function='0x5'/>
    </controller>
    <controller type='sata' index='0'>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x1f' function='0x2'/>
    </controller>
    <controller type='virtio-serial' index='0'>
      <address type='pci' domain='0x0000' bus='0x03' slot='0x00' function='0x0'/>
    </controller>
    <interface type='network'>
      <mac address='{mac_addr}'/>
      <source network='default'/>
      <model type='virtio'/>
      <address type='pci' domain='0x0000' bus='0x01' slot='0x00' function='0x0'/>
    </interface>
    <serial type='pty'>
      <target type='isa-serial' port='0'>
        <model name='isa-serial'/>
      </target>
    </serial>
    <console type='pty'>
      <target type='serial' port='0'/>
    </console>
    <channel type='unix'>
      <target type='virtio' name='org.qemu.guest_agent.0'/>
      <address type='virtio-serial' controller='0' bus='0' port='1'/>
    </channel>
    <channel type='spicevmc'>
      <target type='virtio' name='com.redhat.spice.0'/>
      <address type='virtio-serial' controller='0' bus='0' port='2'/>
    </channel>
    <input type='tablet' bus='usb'>
      <address type='usb' bus='0' port='1'/>
    </input>
    <input type='mouse' bus='ps2'/>
    <input type='keyboard' bus='ps2'/>
    <graphics type='spice' autoport='yes' listen='127.0.0.1'>
      <listen type='address' address='127.0.0.1'/>
      <image compression='off'/>
    </graphics>
    <sound model='ich9'>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x1b' function='0x0'/>
    </sound>
    <audio id='1' type='spice'/>
    <video>
      <model type='virtio' heads='1' primary='yes'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x01' function='0x0'/>
    </video>
    <redirdev bus='usb' type='spicevmc'>
      <address type='usb' bus='0' port='2'/>
    </redirdev>
    <redirdev bus='usb' type='spicevmc'>
      <address type='usb' bus='0' port='3'/>
    </redirdev>
    <watchdog model='itco' action='reset'/>
    <memballoon model='virtio'>
      <address type='pci' domain='0x0000' bus='0x05' slot='0x00' function='0x0'/>
    </memballoon>
    <rng model='virtio'>
      <backend model='random'>/dev/urandom</backend>
      <address type='pci' domain='0x0000' bus='0x06' slot='0x00' function='0x0'/>
    </rng>
{filesystems}  </devices>
</domain>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = "<domain>\
        <disk type='file' device='disk'><source file='/var/lib/libvirt/vm/a.qcow2'/></disk>\
        <disk type='file' device='cdrom'><source file=\"/var/lib/libvirt/vm/a-cloud-init.iso\"/></disk>\
        </domain>";

    #[test]
    fn extracts_disk_and_cdrom_paths() {
        assert_eq!(
            extract_vda_disk_path(XML).as_deref(),
            Some("/var/lib/libvirt/vm/a.qcow2")
        );
        assert_eq!(
            extract_cdrom_path(XML).as_deref(),
            Some("/var/lib/libvirt/vm/a-cloud-init.iso")
        );
        assert_eq!(extract_vda_disk_path("<domain/>"), None);
    }

    #[test]
    fn domain_xml_carries_specs_and_mounts() {
        let specs = VmSpecs {
            memory: 2 * 1024 * 1024,
            vcpus: 4,
            mounts: vec![MountSpec {
                host_path: "/home/user/projects".to_string(),
                tag: "projects".to_string(),
            }],
            ..VmSpecs::default()
        };

        let xml = generate_domain_xml(
            "myvm",
            "/disk.qcow2",
            "/init.iso",
            "52:54:00:aa:bb:cc",
            &specs,
        );

        assert!(xml.contains("<name>myvm</name>"));
        assert!(xml.contains("<memory unit='KiB'>2097152</memory>"));
        assert!(xml.contains("<vcpu placement='static'>4</vcpu>"));
        assert!(xml.contains("<mac address='52:54:00:aa:bb:cc'/>"));
        assert!(xml.contains("<source dir='/home/user/projects'/>"));
        assert_eq!(extract_vda_disk_path(&xml).as_deref(), Some("/disk.qcow2"));
        assert_eq!(extract_cdrom_path(&xml).as_deref(), Some("/init.iso"));
    }

    #[test]
    fn no_mounts_means_no_filesystem_element() {
        let xml = generate_domain_xml("m", "/d", "/i", "52:54:00:00:00:01", &VmSpecs::default());
        assert!(!xml.contains("<filesystem"));
    }
}

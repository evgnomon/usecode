//! Safe wrappers around the libvirt C API.

pub mod sys;

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::path::Path;
use std::ptr;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy)]
pub struct DomainInfo {
    pub memory_kib: u64,
    pub vcpus: u32,
}

#[derive(Debug, Clone)]
pub struct InterfaceAddress {
    pub interface: String,
    pub hwaddr: String,
    pub ip: String,
}

/// Escapes the five XML predefined entities so values can be embedded in generated XML.
pub fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Error handler that suppresses libvirt's own stderr output.
extern "C" fn ignore_error_handler(_user_data: *mut c_void, _error: sys::virErrorPtr) {}

/// Returns the message of the last libvirt error, if any.
pub fn last_error_message() -> Option<String> {
    unsafe {
        let err = sys::virGetLastError();
        if err.is_null() || (*err).message.is_null() {
            return None;
        }
        Some(
            CStr::from_ptr((*err).message)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

fn libvirt_err(op: &str) -> Error {
    match last_error_message() {
        Some(msg) => Error::libvirt(format!("{op}: {msg}")),
        None => Error::libvirt(op.to_string()),
    }
}

fn cstring(value: &str) -> Result<CString> {
    CString::new(value).map_err(|_| Error::vm(format!("value contains a NUL byte: {value}")))
}

/// Copies a NUL-terminated string that stays owned by libvirt; NULL becomes an empty string.
unsafe fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
}

/// Copies a NUL-terminated string owned by libvirt and frees the original.
unsafe fn take_c_string(ptr: *mut c_char) -> String {
    unsafe {
        let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        sys::free(ptr.cast());
        s
    }
}

pub struct Connection {
    pub(crate) ptr: sys::virConnectPtr,
}

impl Connection {
    pub fn open(uri: &str) -> Result<Connection> {
        unsafe { sys::virSetErrorFunc(ptr::null_mut(), ignore_error_handler) };

        let c_uri = cstring(uri)?;
        let conn = unsafe { sys::virConnectOpen(c_uri.as_ptr()) };
        if conn.is_null() {
            error!("Failed to open connection to {uri}");
            return Err(libvirt_err("connect failed"));
        }
        Ok(Connection { ptr: conn })
    }

    /// Makes sure the 'default' network is running and set to autostart.
    pub fn ensure_default_network(&self) -> Result<()> {
        let name = cstring("default")?;
        let net = unsafe { sys::virNetworkLookupByName(self.ptr, name.as_ptr()) };
        if net.is_null() {
            return Ok(());
        }

        unsafe {
            if sys::virNetworkIsActive(net) == 0 {
                sys::virNetworkCreate(net);
            }
            let mut autostart: c_int = 0;
            if sys::virNetworkGetAutostart(net, &mut autostart) == 0 && autostart == 0 {
                sys::virNetworkSetAutostart(net, 1);
            }
            sys::virNetworkFree(net);
        }

        info!("Network 'default' is active and set to autostart");
        Ok(())
    }

    /// Lists running domains, or every defined domain with its state when `all` is set.
    pub fn list_domains(&self, all: bool) -> Result<Vec<String>> {
        let mut domains = Vec::new();

        if all {
            let mut doms: *mut sys::virDomainPtr = ptr::null_mut();
            let n = unsafe { sys::virConnectListAllDomains(self.ptr, &mut doms, 0) };
            if n < 0 {
                return Err(libvirt_err("failed to list domains"));
            }

            unsafe {
                for i in 0..n as isize {
                    let dom = *doms.offset(i);
                    let name = CStr::from_ptr(sys::virDomainGetName(dom)).to_string_lossy();
                    let status = if sys::virDomainIsActive(dom) > 0 {
                        "running"
                    } else {
                        "stopped"
                    };
                    domains.push(format!("{name} ({status})"));
                    sys::virDomainFree(dom);
                }
                sys::free(doms.cast());
            }
        } else {
            let mut ids = [0 as c_int; 128];
            let n = unsafe {
                sys::virConnectListDomains(self.ptr, ids.as_mut_ptr(), ids.len() as c_int)
            };
            if n < 0 {
                return Err(libvirt_err("failed to list domains"));
            }

            for &id in &ids[..n as usize] {
                unsafe {
                    let dom = sys::virDomainLookupByID(self.ptr, id);
                    if dom.is_null() {
                        continue;
                    }
                    let name = CStr::from_ptr(sys::virDomainGetName(dom)).to_string_lossy();
                    domains.push(name.into_owned());
                    sys::virDomainFree(dom);
                }
            }
        }

        Ok(domains)
    }

    /// Looks up the 'vm' storage pool, defining and starting it if it does not exist yet.
    fn ensure_pool(&self) -> Result<Pool> {
        let name = cstring("vm")?;
        let existing = unsafe { sys::virStoragePoolLookupByName(self.ptr, name.as_ptr()) };
        if !existing.is_null() {
            return Ok(Pool { ptr: existing });
        }

        let xml = cstring(
            "<pool type='dir'>\n  <name>vm</name>\n  <target>\n    <path>/var/lib/libvirt/vm</path>\n  </target>\n</pool>",
        )?;
        let pool = unsafe { sys::virStoragePoolDefineXML(self.ptr, xml.as_ptr(), 0) };
        if pool.is_null() {
            return Err(libvirt_err("failed to define storage pool 'vm'"));
        }
        let pool = Pool { ptr: pool };

        unsafe {
            if sys::virStoragePoolBuild(pool.ptr, 0) < 0 {
                return Err(libvirt_err("failed to build storage pool 'vm'"));
            }
            if sys::virStoragePoolCreate(pool.ptr, 0) < 0 {
                return Err(libvirt_err("failed to start storage pool 'vm'"));
            }
            sys::virStoragePoolSetAutostart(pool.ptr, 1);
        }

        info!("Created storage pool 'vm' at /var/lib/libvirt/vm");
        Ok(pool)
    }

    pub fn resize_volume(&self, path: &str, size_bytes: u64) -> Result<()> {
        let pool = self.ensure_pool()?;
        unsafe { sys::virStoragePoolRefresh(pool.ptr, 0) };

        let vol_name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        let c_vol_name = cstring(&vol_name)?;

        unsafe {
            let vol = sys::virStorageVolLookupByName(pool.ptr, c_vol_name.as_ptr());
            if vol.is_null() {
                return Err(libvirt_err(&format!("volume '{vol_name}' not found")));
            }
            let rc = sys::virStorageVolResize(vol, size_bytes, 0);
            sys::virStorageVolFree(vol);
            if rc < 0 {
                return Err(libvirt_err("failed to resize volume"));
            }
        }

        Ok(())
    }

    /// Creates a new qcow2 volume in the 'vm' pool backed by `backing_path`.
    /// The created file will be at `{pool_target}/{vol_name}`.
    pub fn create_volume_with_backing(&self, vol_name: &str, backing_path: &str) -> Result<()> {
        let pool = self.ensure_pool()?;
        unsafe { sys::virStoragePoolRefresh(pool.ptr, 0) };

        let vol_xml = format!(
            "<volume>\n  <name>{}</name>\n  <allocation>0</allocation>\n  <target>\n    <format type='qcow2'/>\n  </target>\n  <backingStore>\n    <path>{}</path>\n    <format type='qcow2'/>\n  </backingStore>\n</volume>",
            xml_escape(vol_name),
            xml_escape(backing_path),
        );
        let c_xml = cstring(&vol_xml)?;

        unsafe {
            let vol = sys::virStorageVolCreateXML(pool.ptr, c_xml.as_ptr(), 0);
            if vol.is_null() {
                if let Some(msg) = last_error_message() {
                    error!("Failed to create volume: {msg}");
                }
                return Err(Error::libvirt("failed to create volume"));
            }
            sys::virStorageVolFree(vol);
        }

        Ok(())
    }

    pub fn lookup_domain(&self, name: &str) -> Result<Domain> {
        let c_name = cstring(name)?;
        let dom = unsafe { sys::virDomainLookupByName(self.ptr, c_name.as_ptr()) };
        if dom.is_null() {
            return Err(Error::libvirt(format!("domain '{name}' not found")));
        }
        Ok(Domain { ptr: dom })
    }

    pub fn volume_size_by_path(&self, path: &str) -> Result<u64> {
        let c_path = cstring(path)?;
        unsafe {
            let vol = sys::virStorageVolLookupByPath(self.ptr, c_path.as_ptr());
            if vol.is_null() {
                return Err(libvirt_err(&format!("volume '{path}' not found")));
            }
            let mut info = std::mem::zeroed::<sys::virStorageVolInfo>();
            let rc = sys::virStorageVolGetInfo(vol, &mut info);
            sys::virStorageVolFree(vol);
            if rc < 0 {
                return Err(libvirt_err("failed to get volume info"));
            }
            Ok(info.capacity)
        }
    }

    pub fn lookup_pool(&self, name: &str) -> Result<Pool> {
        let c_name = cstring(name)?;
        let pool = unsafe { sys::virStoragePoolLookupByName(self.ptr, c_name.as_ptr()) };
        if pool.is_null() {
            return Err(Error::libvirt(format!("storage pool '{name}' not found")));
        }
        Ok(Pool { ptr: pool })
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe { sys::virConnectClose(self.ptr) };
    }
}

pub struct Pool {
    ptr: sys::virStoragePoolPtr,
}

impl Pool {
    pub fn volumes(&self) -> Result<Vec<String>> {
        let num = unsafe { sys::virStoragePoolNumOfVolumes(self.ptr) };
        if num < 0 {
            return Err(libvirt_err("failed to count volumes"));
        }
        if num == 0 {
            return Ok(Vec::new());
        }

        let mut names: Vec<*mut c_char> = vec![ptr::null_mut(); num as usize];
        let n = unsafe { sys::virStoragePoolListVolumes(self.ptr, names.as_mut_ptr(), num) };
        if n < 0 {
            return Err(libvirt_err("failed to list volumes"));
        }

        let mut result = Vec::with_capacity(n as usize);
        for &name in &names[..n as usize] {
            result.push(unsafe { take_c_string(name) });
        }
        Ok(result)
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        unsafe { sys::virStoragePoolFree(self.ptr) };
    }
}

pub struct Domain {
    ptr: sys::virDomainPtr,
}

impl Domain {
    pub fn define_xml(conn: &Connection, xml: &str) -> Result<Domain> {
        let c_xml = cstring(xml)?;
        let dom = unsafe { sys::virDomainDefineXML(conn.ptr, c_xml.as_ptr()) };
        if dom.is_null() {
            error!("Failed to define domain XML");
            return Err(libvirt_err("failed to define domain"));
        }
        Ok(Domain { ptr: dom })
    }

    pub fn create(&self) -> Result<()> {
        if unsafe { sys::virDomainCreate(self.ptr) } < 0 {
            match last_error_message() {
                Some(msg) => error!("Failed to start domain: {msg}"),
                None => error!("Failed to start domain"),
            }
            return Err(Error::libvirt("failed to start domain"));
        }
        Ok(())
    }

    pub fn destroy(&self) -> Result<()> {
        if unsafe { sys::virDomainDestroy(self.ptr) } < 0 {
            return Err(libvirt_err("failed to destroy domain"));
        }
        Ok(())
    }

    /// Requests a graceful ACPI shutdown.
    pub fn shutdown(&self) -> Result<()> {
        if unsafe { sys::virDomainShutdown(self.ptr) } < 0 {
            return Err(libvirt_err("failed to shut down domain"));
        }
        Ok(())
    }

    /// Undefines the domain, also discarding anything that would otherwise make
    /// libvirt refuse: snapshot/checkpoint metadata, a managed save image and
    /// UEFI nvram.  Older libvirt (or drivers that do not know a flag) reject
    /// the whole call, so the flag set is narrowed step by step before falling
    /// back to a plain undefine.
    pub fn undefine(&self) -> Result<()> {
        const ATTEMPTS: [c_uint; 3] = [
            sys::VIR_DOMAIN_UNDEFINE_MANAGED_SAVE
                | sys::VIR_DOMAIN_UNDEFINE_SNAPSHOTS_METADATA
                | sys::VIR_DOMAIN_UNDEFINE_CHECKPOINTS_METADATA
                | sys::VIR_DOMAIN_UNDEFINE_NVRAM,
            sys::VIR_DOMAIN_UNDEFINE_MANAGED_SAVE
                | sys::VIR_DOMAIN_UNDEFINE_SNAPSHOTS_METADATA
                | sys::VIR_DOMAIN_UNDEFINE_CHECKPOINTS_METADATA,
            sys::VIR_DOMAIN_UNDEFINE_MANAGED_SAVE
                | sys::VIR_DOMAIN_UNDEFINE_SNAPSHOTS_METADATA,
        ];

        for flags in ATTEMPTS {
            if unsafe { sys::virDomainUndefineFlags(self.ptr, flags) } >= 0 {
                return Ok(());
            }
        }

        if unsafe { sys::virDomainUndefine(self.ptr) } < 0 {
            return Err(libvirt_err("failed to undefine domain"));
        }
        Ok(())
    }

    pub fn name(&self) -> String {
        unsafe {
            CStr::from_ptr(sys::virDomainGetName(self.ptr))
                .to_string_lossy()
                .into_owned()
        }
    }

    pub fn is_active(&self) -> bool {
        unsafe { sys::virDomainIsActive(self.ptr) == 1 }
    }

    pub fn info(&self) -> Result<DomainInfo> {
        unsafe {
            let mut info = std::mem::zeroed::<sys::virDomainInfo>();
            if sys::virDomainGetInfo(self.ptr, &mut info) < 0 {
                return Err(libvirt_err("failed to get domain info"));
            }
            Ok(DomainInfo {
                memory_kib: info.maxMem as u64,
                vcpus: info.nrVirtCpu as u32,
            })
        }
    }

    /// Returns the IP addresses of the interface whose MAC matches `mac_addr`.
    pub fn ip_addresses(&self, mac_addr: &str) -> Result<Vec<String>> {
        let addresses = self.interface_addresses()?;
        Ok(addresses
            .into_iter()
            .filter(|addr| addr.hwaddr == mac_addr)
            .map(|addr| addr.ip)
            .collect())
    }

    /// Returns every address libvirt knows about for this domain.
    /// The guest agent is queried first, falling back to DHCP leases.
    pub fn interface_addresses(&self) -> Result<Vec<InterfaceAddress>> {
        let mut ifaces: *mut sys::virDomainInterfacePtr = ptr::null_mut();

        let mut nifaces = unsafe {
            sys::virDomainInterfaceAddresses(
                self.ptr,
                &mut ifaces,
                sys::VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_AGENT,
                0,
            )
        };
        if nifaces <= 0 {
            nifaces = unsafe {
                sys::virDomainInterfaceAddresses(
                    self.ptr,
                    &mut ifaces,
                    sys::VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_LEASE,
                    0,
                )
            };
        }

        if nifaces <= 0 || ifaces.is_null() {
            return Err(Error::vm("no interfaces found"));
        }

        let mut addresses = Vec::new();
        unsafe {
            for i in 0..nifaces as isize {
                let iface = &*(*ifaces.offset(i));
                let name = cstr_to_string(iface.name);
                let hwaddr = cstr_to_string(iface.hwaddr);

                for j in 0..iface.naddrs as isize {
                    let addr = &*iface.addrs.offset(j);
                    if addr.type_ == sys::VIR_IP_ADDR_TYPE_IPV4
                        || addr.type_ == sys::VIR_IP_ADDR_TYPE_IPV6
                    {
                        addresses.push(InterfaceAddress {
                            interface: name.clone(),
                            hwaddr: hwaddr.clone(),
                            ip: cstr_to_string(addr.addr),
                        });
                    }
                }
            }

            for i in 0..nifaces as isize {
                sys::virDomainInterfaceFree(*ifaces.offset(i));
            }
            sys::free(ifaces.cast());
        }

        Ok(addresses)
    }

    pub fn create_snapshot(&self, name: &str) -> Result<Snapshot> {
        let xml = format!(
            "<domainsnapshot><name>{}</name></domainsnapshot>",
            xml_escape(name)
        );
        let c_xml = cstring(&xml)?;
        let snap = unsafe { sys::virDomainSnapshotCreateXML(self.ptr, c_xml.as_ptr(), 0) };
        if snap.is_null() {
            return Err(libvirt_err("failed to create snapshot"));
        }
        Ok(Snapshot { ptr: snap })
    }

    pub fn revert_to_snapshot(&self, name: &str) -> Result<()> {
        let snap = self.lookup_snapshot(name)?;
        if unsafe { sys::virDomainRevertToSnapshot(snap.ptr, 0) } < 0 {
            return Err(libvirt_err("failed to revert to snapshot"));
        }
        Ok(())
    }

    pub fn delete_snapshot(&self, name: &str) -> Result<()> {
        let snap = self.lookup_snapshot(name)?;
        snap.delete()
    }

    fn lookup_snapshot(&self, name: &str) -> Result<Snapshot> {
        let c_name = cstring(name)?;
        let snap = unsafe { sys::virDomainSnapshotLookupByName(self.ptr, c_name.as_ptr(), 0) };
        if snap.is_null() {
            return Err(Error::libvirt(format!("snapshot '{name}' not found")));
        }
        Ok(Snapshot { ptr: snap })
    }

    pub fn list_snapshots(&self) -> Result<Vec<String>> {
        let num = unsafe { sys::virDomainSnapshotNum(self.ptr, 0) };
        if num < 0 {
            return Err(libvirt_err("failed to count snapshots"));
        }
        if num == 0 {
            return Ok(Vec::new());
        }

        let mut names: Vec<*mut c_char> = vec![ptr::null_mut(); num as usize];
        let n = unsafe { sys::virDomainSnapshotListNames(self.ptr, names.as_mut_ptr(), num, 0) };
        if n < 0 {
            return Err(libvirt_err("failed to list snapshots"));
        }

        let mut result = Vec::with_capacity(n as usize);
        for &name in &names[..n as usize] {
            result.push(unsafe { take_c_string(name) });
        }
        Ok(result)
    }

    /// Returns the domain's persistent (inactive) XML description.
    pub fn xml(&self) -> Result<String> {
        let raw = unsafe { sys::virDomainGetXMLDesc(self.ptr, sys::VIR_DOMAIN_XML_INACTIVE) };
        if raw.is_null() {
            return Err(libvirt_err("failed to get domain XML"));
        }
        Ok(unsafe { take_c_string(raw) })
    }

    /// Attaches a device described by `xml` to the domain.
    /// If the domain is active the device is attached live and the persistent config is updated.
    /// If the domain is inactive only the persistent config is updated.
    pub fn attach_device(&self, xml: &str) -> Result<()> {
        let c_xml = cstring(xml)?;
        let flags = if self.is_active() {
            sys::VIR_DOMAIN_AFFECT_LIVE | sys::VIR_DOMAIN_AFFECT_CONFIG
        } else {
            sys::VIR_DOMAIN_AFFECT_CONFIG
        };

        if unsafe { sys::virDomainAttachDeviceFlags(self.ptr, c_xml.as_ptr(), flags) } < 0 {
            if let Some(msg) = last_error_message() {
                error!("Failed to attach device: {msg}");
            }
            return Err(Error::libvirt("failed to attach device"));
        }
        Ok(())
    }

    /// Sets the memory allocation for the domain.
    /// Always updates the persistent config. If the domain is running, also attempts
    /// a live update (requires guest balloon driver); returns true if the live update
    /// succeeded so callers can tell the user whether a restart is needed.
    pub fn set_memory(&self, memory_kib: u64) -> Result<bool> {
        let mem_kib = memory_kib as std::os::raw::c_ulong;
        let config = sys::VIR_DOMAIN_AFFECT_CONFIG;
        let config_max = sys::VIR_DOMAIN_AFFECT_CONFIG | sys::VIR_DOMAIN_MEM_MAXIMUM;

        unsafe {
            if sys::virDomainSetMemoryFlags(self.ptr, mem_kib, config_max) < 0 {
                return Err(libvirt_err("failed to set maximum memory"));
            }
            if sys::virDomainSetMemoryFlags(self.ptr, mem_kib, config) < 0 {
                return Err(libvirt_err("failed to set memory"));
            }

            if !self.is_active() {
                return Ok(true);
            }

            let live = sys::VIR_DOMAIN_AFFECT_LIVE;
            let live_max = sys::VIR_DOMAIN_AFFECT_LIVE | sys::VIR_DOMAIN_MEM_MAXIMUM;
            if sys::virDomainSetMemoryFlags(self.ptr, mem_kib, live_max) < 0 {
                return Ok(false);
            }
            if sys::virDomainSetMemoryFlags(self.ptr, mem_kib, live) < 0 {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Creates an external, disk-only snapshot at `snapshot_path` for disk `disk_device`
    /// (e.g. "vda").  Uses DISK_ONLY | ATOMIC | NO_METADATA so libvirt does not
    /// track it as a named snapshot.  The snapshot file becomes the new active
    /// overlay for the domain; the original disk becomes the read-only backing store.
    pub fn create_external_disk_snapshot(
        &self,
        disk_device: &str,
        snapshot_path: &str,
    ) -> Result<()> {
        let xml = format!(
            "<domainsnapshot>\n  <disks>\n    <disk name='{}' snapshot='external'>\n      <source file='{}'/>\n    </disk>\n  </disks>\n</domainsnapshot>",
            xml_escape(disk_device),
            xml_escape(snapshot_path),
        );
        let c_xml = cstring(&xml)?;

        let flags = sys::VIR_DOMAIN_SNAPSHOT_CREATE_DISK_ONLY
            | sys::VIR_DOMAIN_SNAPSHOT_CREATE_ATOMIC
            | sys::VIR_DOMAIN_SNAPSHOT_CREATE_NO_METADATA;

        let snap = unsafe { sys::virDomainSnapshotCreateXML(self.ptr, c_xml.as_ptr(), flags) };
        if snap.is_null() {
            if let Some(msg) = last_error_message() {
                error!("External snapshot failed: {msg}");
            }
            return Err(Error::libvirt("failed to create external snapshot"));
        }
        unsafe { sys::virDomainSnapshotFree(snap) };
        Ok(())
    }
}

impl Drop for Domain {
    fn drop(&mut self) {
        unsafe { sys::virDomainFree(self.ptr) };
    }
}

pub struct Snapshot {
    ptr: sys::virDomainSnapshotPtr,
}

impl Snapshot {
    pub fn delete(&self) -> Result<()> {
        if unsafe { sys::virDomainSnapshotDelete(self.ptr, 0) } < 0 {
            return Err(libvirt_err("failed to delete snapshot"));
        }
        Ok(())
    }

    pub fn name(&self) -> String {
        unsafe {
            CStr::from_ptr(sys::virDomainSnapshotGetName(self.ptr))
                .to_string_lossy()
                .into_owned()
        }
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        unsafe { sys::virDomainSnapshotFree(self.ptr) };
    }
}

#[cfg(test)]
mod tests {
    use super::xml_escape;

    #[test]
    fn escapes_xml_entities() {
        assert_eq!(
            xml_escape("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
        assert_eq!(xml_escape("plain"), "plain");
    }
}

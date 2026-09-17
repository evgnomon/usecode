//! Hand-written FFI declarations for the subset of libvirt that vm uses.
#![allow(non_camel_case_types, non_snake_case)]

use std::os::raw::{c_char, c_int, c_uchar, c_uint, c_ulong, c_ulonglong, c_ushort, c_void};

pub enum virConnect {}
pub enum virDomain {}
pub enum virNetwork {}
pub enum virStoragePool {}
pub enum virStorageVol {}
pub enum virDomainSnapshot {}

pub type virConnectPtr = *mut virConnect;
pub type virDomainPtr = *mut virDomain;
pub type virNetworkPtr = *mut virNetwork;
pub type virStoragePoolPtr = *mut virStoragePool;
pub type virStorageVolPtr = *mut virStorageVol;
pub type virDomainSnapshotPtr = *mut virDomainSnapshot;
pub type virErrorPtr = *mut virError;
pub type virErrorFunc = extern "C" fn(userData: *mut c_void, error: virErrorPtr);

#[repr(C)]
pub struct virError {
    pub code: c_int,
    pub domain: c_int,
    pub message: *mut c_char,
    pub level: c_int,
    pub conn: virConnectPtr,
    pub dom: virDomainPtr,
    pub str1: *mut c_char,
    pub str2: *mut c_char,
    pub str3: *mut c_char,
    pub int1: c_int,
    pub int2: c_int,
    pub net: *mut c_void,
}

#[repr(C)]
pub struct virDomainInfo {
    pub state: c_uchar,
    pub maxMem: c_ulong,
    pub memory: c_ulong,
    pub nrVirtCpu: c_ushort,
    pub cpuTime: c_ulonglong,
}

#[repr(C)]
pub struct virStorageVolInfo {
    pub type_: c_int,
    pub capacity: c_ulonglong,
    pub allocation: c_ulonglong,
}

#[repr(C)]
pub struct virDomainIPAddress {
    pub type_: c_int,
    pub addr: *mut c_char,
    pub prefix: c_uint,
}

#[repr(C)]
pub struct virDomainInterface {
    pub name: *mut c_char,
    pub hwaddr: *mut c_char,
    pub naddrs: c_uint,
    pub addrs: *mut virDomainIPAddress,
}

pub type virDomainInterfacePtr = *mut virDomainInterface;

pub const VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_LEASE: c_uint = 0;
pub const VIR_DOMAIN_INTERFACE_ADDRESSES_SRC_AGENT: c_uint = 1;

pub const VIR_IP_ADDR_TYPE_IPV4: c_int = 0;
pub const VIR_IP_ADDR_TYPE_IPV6: c_int = 1;

pub const VIR_DOMAIN_XML_INACTIVE: c_uint = 1 << 1;

pub const VIR_DOMAIN_AFFECT_LIVE: c_uint = 1 << 0;
pub const VIR_DOMAIN_AFFECT_CONFIG: c_uint = 1 << 1;
pub const VIR_DOMAIN_MEM_MAXIMUM: c_uint = 1 << 2;

pub const VIR_DOMAIN_UNDEFINE_MANAGED_SAVE: c_uint = 1 << 0;
pub const VIR_DOMAIN_UNDEFINE_SNAPSHOTS_METADATA: c_uint = 1 << 1;
pub const VIR_DOMAIN_UNDEFINE_NVRAM: c_uint = 1 << 2;
pub const VIR_DOMAIN_UNDEFINE_CHECKPOINTS_METADATA: c_uint = 1 << 4;

pub const VIR_DOMAIN_SNAPSHOT_CREATE_NO_METADATA: c_uint = 1 << 2;
pub const VIR_DOMAIN_SNAPSHOT_CREATE_DISK_ONLY: c_uint = 1 << 4;
pub const VIR_DOMAIN_SNAPSHOT_CREATE_ATOMIC: c_uint = 1 << 7;

unsafe extern "C" {
    pub fn free(ptr: *mut c_void);

    pub fn virSetErrorFunc(userData: *mut c_void, handler: virErrorFunc);
    pub fn virGetLastError() -> virErrorPtr;

    pub fn virConnectOpen(name: *const c_char) -> virConnectPtr;
    pub fn virConnectClose(conn: virConnectPtr) -> c_int;
    pub fn virConnectListDomains(conn: virConnectPtr, ids: *mut c_int, maxids: c_int) -> c_int;
    pub fn virConnectListAllDomains(
        conn: virConnectPtr,
        domains: *mut *mut virDomainPtr,
        flags: c_uint,
    ) -> c_int;

    pub fn virNetworkLookupByName(conn: virConnectPtr, name: *const c_char) -> virNetworkPtr;
    pub fn virNetworkFree(network: virNetworkPtr) -> c_int;
    pub fn virNetworkIsActive(network: virNetworkPtr) -> c_int;
    pub fn virNetworkCreate(network: virNetworkPtr) -> c_int;
    pub fn virNetworkGetAutostart(network: virNetworkPtr, autostart: *mut c_int) -> c_int;
    pub fn virNetworkSetAutostart(network: virNetworkPtr, autostart: c_int) -> c_int;

    pub fn virDomainLookupByID(conn: virConnectPtr, id: c_int) -> virDomainPtr;
    pub fn virDomainLookupByName(conn: virConnectPtr, name: *const c_char) -> virDomainPtr;
    pub fn virDomainDefineXML(conn: virConnectPtr, xml: *const c_char) -> virDomainPtr;
    pub fn virDomainFree(domain: virDomainPtr) -> c_int;
    pub fn virDomainGetName(domain: virDomainPtr) -> *const c_char;
    pub fn virDomainIsActive(domain: virDomainPtr) -> c_int;
    pub fn virDomainCreate(domain: virDomainPtr) -> c_int;
    pub fn virDomainDestroy(domain: virDomainPtr) -> c_int;
    pub fn virDomainShutdown(domain: virDomainPtr) -> c_int;
    pub fn virDomainUndefine(domain: virDomainPtr) -> c_int;
    pub fn virDomainUndefineFlags(domain: virDomainPtr, flags: c_uint) -> c_int;
    pub fn virDomainGetInfo(domain: virDomainPtr, info: *mut virDomainInfo) -> c_int;
    pub fn virDomainGetXMLDesc(domain: virDomainPtr, flags: c_uint) -> *mut c_char;
    pub fn virDomainAttachDeviceFlags(
        domain: virDomainPtr,
        xml: *const c_char,
        flags: c_uint,
    ) -> c_int;
    pub fn virDomainSetMemoryFlags(domain: virDomainPtr, memory: c_ulong, flags: c_uint) -> c_int;
    pub fn virDomainInterfaceAddresses(
        domain: virDomainPtr,
        ifaces: *mut *mut virDomainInterfacePtr,
        source: c_uint,
        flags: c_uint,
    ) -> c_int;
    pub fn virDomainInterfaceFree(iface: virDomainInterfacePtr);

    pub fn virDomainSnapshotCreateXML(
        domain: virDomainPtr,
        xmlDesc: *const c_char,
        flags: c_uint,
    ) -> virDomainSnapshotPtr;
    pub fn virDomainSnapshotLookupByName(
        domain: virDomainPtr,
        name: *const c_char,
        flags: c_uint,
    ) -> virDomainSnapshotPtr;
    pub fn virDomainSnapshotDelete(snapshot: virDomainSnapshotPtr, flags: c_uint) -> c_int;
    pub fn virDomainSnapshotFree(snapshot: virDomainSnapshotPtr) -> c_int;
    pub fn virDomainSnapshotGetName(snapshot: virDomainSnapshotPtr) -> *const c_char;
    pub fn virDomainSnapshotNum(domain: virDomainPtr, flags: c_uint) -> c_int;
    pub fn virDomainSnapshotListNames(
        domain: virDomainPtr,
        names: *mut *mut c_char,
        nameslen: c_int,
        flags: c_uint,
    ) -> c_int;
    pub fn virDomainRevertToSnapshot(snapshot: virDomainSnapshotPtr, flags: c_uint) -> c_int;

    pub fn virStoragePoolLookupByName(
        conn: virConnectPtr,
        name: *const c_char,
    ) -> virStoragePoolPtr;
    pub fn virStoragePoolDefineXML(
        conn: virConnectPtr,
        xml: *const c_char,
        flags: c_uint,
    ) -> virStoragePoolPtr;
    pub fn virStoragePoolBuild(pool: virStoragePoolPtr, flags: c_uint) -> c_int;
    pub fn virStoragePoolCreate(pool: virStoragePoolPtr, flags: c_uint) -> c_int;
    pub fn virStoragePoolRefresh(pool: virStoragePoolPtr, flags: c_uint) -> c_int;
    pub fn virStoragePoolSetAutostart(pool: virStoragePoolPtr, autostart: c_int) -> c_int;
    pub fn virStoragePoolFree(pool: virStoragePoolPtr) -> c_int;
    pub fn virStoragePoolNumOfVolumes(pool: virStoragePoolPtr) -> c_int;
    pub fn virStoragePoolListVolumes(
        pool: virStoragePoolPtr,
        names: *mut *mut c_char,
        maxnames: c_int,
    ) -> c_int;

    pub fn virStorageVolLookupByName(
        pool: virStoragePoolPtr,
        name: *const c_char,
    ) -> virStorageVolPtr;
    pub fn virStorageVolLookupByPath(conn: virConnectPtr, path: *const c_char) -> virStorageVolPtr;
    pub fn virStorageVolCreateXML(
        pool: virStoragePoolPtr,
        xml: *const c_char,
        flags: c_uint,
    ) -> virStorageVolPtr;
    pub fn virStorageVolResize(
        vol: virStorageVolPtr,
        capacity: c_ulonglong,
        flags: c_uint,
    ) -> c_int;
    pub fn virStorageVolGetInfo(vol: virStorageVolPtr, info: *mut virStorageVolInfo) -> c_int;
    pub fn virStorageVolFree(vol: virStorageVolPtr) -> c_int;
}

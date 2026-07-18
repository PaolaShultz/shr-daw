//! Shared, minimal JACK dynamic-loading boundary.
//!
//! JACK is a system dependency rather than a Rust crate dependency in this
//! project. All symbol loading, client lifetime, port registration, graph
//! connection, and deactivation lives here so real-time clients share one
//! reviewed FFI contract. Callers still own their callback data and must keep
//! it alive until [`Client::deactivate`] has returned.

use anyhow::{bail, Context, Result};
use libc::{c_char, c_int, c_uint, c_ulong, c_void};
use std::ffi::{CStr, CString};

const JACK_DEFAULT_AUDIO_TYPE: &[u8] = b"32 bit float mono audio\0";
const JACK_PORT_IS_INPUT: c_ulong = 1;
const JACK_PORT_IS_OUTPUT: c_ulong = 2;
const JACK_NO_START_SERVER: c_uint = 1;

pub(crate) type ProcessCallback = unsafe extern "C" fn(c_uint, *mut c_void) -> c_int;
pub(crate) type ShutdownCallback = unsafe extern "C" fn(*mut c_void);
pub(crate) type PortGetBuffer = unsafe extern "C" fn(*mut Port, c_uint) -> *mut c_void;

#[repr(C)]
struct OpaqueClient {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct Port {
    _private: [u8; 0],
}

type ClientOpen = unsafe extern "C" fn(*const c_char, c_uint, *mut c_uint) -> *mut OpaqueClient;
type ClientClose = unsafe extern "C" fn(*mut OpaqueClient) -> c_int;
type PortRegister = unsafe extern "C" fn(
    *mut OpaqueClient,
    *const c_char,
    *const c_char,
    c_ulong,
    c_ulong,
) -> *mut Port;
type SetProcess = unsafe extern "C" fn(*mut OpaqueClient, ProcessCallback, *mut c_void) -> c_int;
type OnShutdown = unsafe extern "C" fn(*mut OpaqueClient, ShutdownCallback, *mut c_void);
type Activate = unsafe extern "C" fn(*mut OpaqueClient) -> c_int;
type Deactivate = unsafe extern "C" fn(*mut OpaqueClient) -> c_int;
type Connect = unsafe extern "C" fn(*mut OpaqueClient, *const c_char, *const c_char) -> c_int;
type Disconnect = unsafe extern "C" fn(*mut OpaqueClient, *const c_char, *const c_char) -> c_int;
type PortName = unsafe extern "C" fn(*const Port) -> *const c_char;
type SampleRate = unsafe extern "C" fn(*const OpaqueClient) -> c_uint;

struct Api {
    handle: *mut c_void,
    client_close: ClientClose,
    port_register: PortRegister,
    set_process: SetProcess,
    on_shutdown: OnShutdown,
    activate: Activate,
    deactivate: Deactivate,
    connect: Connect,
    disconnect: Disconnect,
    port_name: PortName,
    sample_rate: SampleRate,
    port_get_buffer: PortGetBuffer,
}

// The loaded function pointers are immutable after construction. JACK allows
// client ownership to move to the application's non-real-time owner thread.
unsafe impl Send for Api {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PortDirection {
    Input,
    Output,
}

pub(crate) struct Client {
    client: *mut OpaqueClient,
    api: Api,
    active: bool,
}

unsafe impl Send for Client {}

impl Client {
    pub(crate) fn open(name: &str) -> Result<Self> {
        let name = CString::new(name).context("JACK client name contains a NUL byte")?;
        // SAFETY: the handle and every required symbol remain owned by `Api`
        // until after jack_client_close in Drop.
        unsafe {
            let handle = libc::dlopen(c"libjack.so.0".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if handle.is_null() {
                bail!("JACK library unavailable");
            }
            let loaded = (|| -> Result<(ClientOpen, Api)> {
                Ok((
                    symbol(handle, b"jack_client_open\0")?,
                    Api {
                        handle,
                        client_close: symbol(handle, b"jack_client_close\0")?,
                        port_register: symbol(handle, b"jack_port_register\0")?,
                        set_process: symbol(handle, b"jack_set_process_callback\0")?,
                        on_shutdown: symbol(handle, b"jack_on_shutdown\0")?,
                        activate: symbol(handle, b"jack_activate\0")?,
                        deactivate: symbol(handle, b"jack_deactivate\0")?,
                        connect: symbol(handle, b"jack_connect\0")?,
                        disconnect: symbol(handle, b"jack_disconnect\0")?,
                        port_name: symbol(handle, b"jack_port_name\0")?,
                        sample_rate: symbol(handle, b"jack_get_sample_rate\0")?,
                        port_get_buffer: symbol(handle, b"jack_port_get_buffer\0")?,
                    },
                ))
            })();
            let (open, api) = match loaded {
                Ok(loaded) => loaded,
                Err(error) => {
                    libc::dlclose(handle);
                    return Err(error);
                }
            };
            let mut status = 0;
            let client = open(name.as_ptr(), JACK_NO_START_SERVER, &mut status);
            if client.is_null() {
                libc::dlclose(handle);
                bail!("JACK server unavailable (status {status})");
            }
            Ok(Self {
                client,
                api,
                active: false,
            })
        }
    }

    pub(crate) fn sample_rate(&self) -> u32 {
        // SAFETY: `self.client` remains valid for the lifetime of `self`.
        unsafe { (self.api.sample_rate)(self.client) }
    }

    pub(crate) fn register_audio_port(
        &self,
        name: &str,
        direction: PortDirection,
    ) -> Result<*mut Port> {
        let name = CString::new(name).context("JACK port name contains a NUL byte")?;
        let flags = match direction {
            PortDirection::Input => JACK_PORT_IS_INPUT,
            PortDirection::Output => JACK_PORT_IS_OUTPUT,
        };
        // SAFETY: all pointers are valid for the duration of the call and the
        // returned port is owned by the JACK client.
        let port = unsafe {
            (self.api.port_register)(
                self.client,
                name.as_ptr(),
                JACK_DEFAULT_AUDIO_TYPE.as_ptr().cast(),
                flags,
                0,
            )
        };
        if port.is_null() {
            bail!("register JACK {direction:?} port {name:?}");
        }
        Ok(port)
    }

    /// Register the process callback before activating the client.
    ///
    /// The caller must keep `argument` valid and pinned until `deactivate`
    /// returns. The callback itself must obey JACK's real-time restrictions.
    pub(crate) unsafe fn set_process_callback(
        &self,
        callback: ProcessCallback,
        argument: *mut c_void,
    ) -> Result<()> {
        // SAFETY: validity of the callback and opaque argument is the caller's
        // contract documented by this method.
        if unsafe { (self.api.set_process)(self.client, callback, argument) } != 0 {
            bail!("set JACK process callback");
        }
        Ok(())
    }

    /// Register a notification that may only set lock-free state. As with the
    /// process callback, `argument` must remain valid until deactivation.
    pub(crate) unsafe fn set_shutdown_callback(
        &self,
        callback: ShutdownCallback,
        argument: *mut c_void,
    ) {
        // SAFETY: validity of the callback and opaque argument is the caller's
        // contract documented by this method.
        unsafe { (self.api.on_shutdown)(self.client, callback, argument) };
    }

    pub(crate) fn port_get_buffer(&self) -> PortGetBuffer {
        self.api.port_get_buffer
    }

    pub(crate) fn activate(&mut self) -> Result<()> {
        if self.active {
            return Ok(());
        }
        // SAFETY: the client is open and all caller-required setup precedes
        // activation.
        if unsafe { (self.api.activate)(self.client) } != 0 {
            bail!("activate JACK client");
        }
        self.active = true;
        Ok(())
    }

    pub(crate) fn connect_port_to_external(
        &self,
        source: *mut Port,
        destination: &str,
    ) -> Result<()> {
        let source = self.port_name(source)?;
        let destination =
            CString::new(destination).context("JACK port name contains a NUL byte")?;
        self.connect(source.as_ptr(), destination.as_ptr())
    }

    pub(crate) fn connect_external_to_port(
        &self,
        source: &str,
        destination: *mut Port,
    ) -> Result<()> {
        let source = CString::new(source).context("JACK port name contains a NUL byte")?;
        let destination = self.port_name(destination)?;
        self.connect(source.as_ptr(), destination.as_ptr())
    }

    pub(crate) fn port_name_string(&self, port: *mut Port) -> Result<String> {
        Ok(self.port_name(port)?.to_string_lossy().into_owned())
    }

    /// Ensure an exact named connection exists. The boolean is true only when
    /// this call created it, allowing transactional rollback to preserve a
    /// connection that already existed.
    pub(crate) fn ensure_connection(&self, source: &str, destination: &str) -> Result<bool> {
        let source = CString::new(source).context("JACK port name contains a NUL byte")?;
        let destination =
            CString::new(destination).context("JACK port name contains a NUL byte")?;
        let status =
            unsafe { (self.api.connect)(self.client, source.as_ptr(), destination.as_ptr()) };
        if status == 0 {
            Ok(true)
        } else if status == libc::EEXIST {
            Ok(false)
        } else {
            bail!("connect JACK ports (status {status})")
        }
    }

    /// Remove an exact named connection. JACK does not define a distinct
    /// already-absent status here, so every non-zero result remains a failure.
    pub(crate) fn remove_connection(&self, source: &str, destination: &str) -> Result<bool> {
        let source = CString::new(source).context("JACK port name contains a NUL byte")?;
        let destination =
            CString::new(destination).context("JACK port name contains a NUL byte")?;
        let status =
            unsafe { (self.api.disconnect)(self.client, source.as_ptr(), destination.as_ptr()) };
        if status == 0 {
            Ok(true)
        } else {
            bail!("disconnect JACK ports (status {status})")
        }
    }

    pub(crate) fn deactivate(&mut self) {
        if self.active {
            // SAFETY: JACK permits deactivation of an active open client.
            unsafe { (self.api.deactivate)(self.client) };
            self.active = false;
        }
    }

    fn port_name(&self, port: *mut Port) -> Result<&CStr> {
        if port.is_null() {
            bail!("JACK returned a null port");
        }
        // SAFETY: the port belongs to this live client; JACK owns the string.
        let name = unsafe { (self.api.port_name)(port) };
        if name.is_null() {
            bail!("JACK returned an unnamed port");
        }
        Ok(unsafe { CStr::from_ptr(name) })
    }

    fn connect(&self, source: *const c_char, destination: *const c_char) -> Result<()> {
        // SAFETY: both arguments are live NUL-terminated port names.
        let status = unsafe { (self.api.connect)(self.client, source, destination) };
        if status != 0 && status != libc::EEXIST {
            bail!("connect JACK ports (status {status})");
        }
        Ok(())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.deactivate();
        // SAFETY: close the client before unloading the library that provides
        // the close function.
        unsafe {
            (self.api.client_close)(self.client);
            libc::dlclose(self.api.handle);
        }
    }
}

unsafe fn symbol<T: Copy>(handle: *mut c_void, name: &[u8]) -> Result<T> {
    // SAFETY: `name` is NUL terminated and `handle` is a live dlopen handle.
    let pointer = unsafe { libc::dlsym(handle, name.as_ptr().cast()) };
    if pointer.is_null() {
        let label = CStr::from_bytes_with_nul(name)
            .map(|name| name.to_string_lossy())
            .unwrap_or_else(|_| "unknown".into());
        bail!("JACK symbol unavailable: {label}");
    }
    // SAFETY: each call site requests the exact function signature from JACK's
    // public C API.
    Ok(unsafe { std::mem::transmute_copy(&pointer) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jack_abi_types_match_platform_contract() {
        assert_eq!(std::mem::size_of::<c_ulong>(), std::mem::size_of::<usize>());
        assert_eq!(JACK_PORT_IS_INPUT, 1);
        assert_eq!(JACK_PORT_IS_OUTPUT, 2);
        assert_eq!(JACK_DEFAULT_AUDIO_TYPE.last(), Some(&0));
    }
}

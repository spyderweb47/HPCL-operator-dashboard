//! Windows 7 only. build.rs delay-loads the DLLs below; when Windows 7 can't provide one,
//! the delay-load helper asks this hook for a replacement instead of crashing.
//! - `combase.dll` is Windows 8+; the same COM functions are in `ole32.dll` on Windows 7.
//! - `EventSetInformation` (advapi32, used by the WebView2 loader for telemetry) is Windows 8+.
//! - `RoOriginateErrorW` (extra error details in the `windows` crate) is Windows 8+.

use std::ffi::{c_char, c_void, CStr};

const DLI_FAIL_LOAD_LIB: u32 = 3;
const DLI_FAIL_GET_PROC: u32 = 4;

#[repr(C)]
struct DelayLoadProc {
    import_by_name: i32,
    name_or_ordinal: *const c_char,
}

#[repr(C)]
struct DelayLoadInfo {
    cb: u32,
    pidd: *const c_void,
    ppfn: *mut *const c_void,
    dll: *const c_char,
    dlp: DelayLoadProc,
    hmod_cur: *mut c_void,
    pfn_cur: *const c_void,
    last_error: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: *const c_char) -> *mut c_void;
    fn GetModuleHandleA(name: *const c_char) -> *mut c_void;
}

type Hook = unsafe extern "system" fn(u32, *const DelayLoadInfo) -> *const c_void;

#[used]
#[no_mangle]
static __pfnDliFailureHook2: Hook = failure_hook;

unsafe extern "system" fn failure_hook(notify: u32, info: *const DelayLoadInfo) -> *const c_void {
    let Some(info) = info.as_ref() else { return std::ptr::null() };
    let dll = CStr::from_ptr(info.dll).to_string_lossy().to_ascii_lowercase();
    match notify {
        DLI_FAIL_LOAD_LIB if dll == "combase.dll" => LoadLibraryA(c"ole32.dll".as_ptr()) as _,
        // Any loaded module will do: the function lookup then fails and lands below.
        DLI_FAIL_LOAD_LIB if dll.starts_with("api-ms-win-core-winrt-error") => GetModuleHandleA(c"kernel32.dll".as_ptr()) as _,
        DLI_FAIL_GET_PROC if info.dlp.import_by_name != 0 => match CStr::from_ptr(info.dlp.name_or_ordinal).to_bytes() {
            b"EventSetInformation" => event_set_information as *const c_void,
            b"RoOriginateErrorW" => ro_originate_error_w as *const c_void,
            _ => std::ptr::null(),
        },
        _ => std::ptr::null(),
    }
}

unsafe extern "system" fn event_set_information(_handle: u64, _class: i32, _info: *mut c_void, _len: u32) -> u32 {
    50 // ERROR_NOT_SUPPORTED
}

unsafe extern "system" fn ro_originate_error_w(_error: i32, _len: u32, _message: *const u16) -> i32 {
    0 // FALSE: no extra error details recorded
}

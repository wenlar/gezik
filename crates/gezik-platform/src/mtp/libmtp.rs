//! macOS and Linux (spec 16 §10.3, §10.4): a phone through libmtp, loaded with `dlopen` the
//! first time one is opened (never linked). macOS looks for it in settings, then Homebrew's,
//! then Gezik's download; Linux in settings, then the system's `libmtp.so.9`. libmtp keeps no
//! version to ask at run time: every function used must be there (1.1.17 or newer has them).
//!
//! Object ids are `<storage>:<object>` (listing and writing need the storage), a storage's own
//! `<storage>:4294967295` (libmtp's root).

#![cfg_attr(not(unix), allow(dead_code))]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use super::{Object, Session};

/// `LIBMTP_FILES_AND_FOLDERS_ROOT`: the top of a storage.
const ROOT: u32 = 0xffff_ffff;
const FOLDER: c_int = 0;
/// `LIBMTP_FILETYPE_UNKNOWN`: the phone decides by the name.
const UNKNOWN: c_int = 44;
const HANDLER_OK: u16 = 0;
const HANDLER_ERROR: u16 = 1;
const HANDLER_CANCEL: u16 = 2;

/// Homebrew's libmtp (Apple silicon, Intel).
const HOMEBREW: [&str; 2] = ["/opt/homebrew/lib/libmtp.9.dylib", "/usr/local/lib/libmtp.9.dylib"];
const SYSTEM: &str = "libmtp.so.9";

#[repr(C)]
struct DeviceEntry {
    vendor: *mut c_char,
    vendor_id: u16,
    product: *mut c_char,
    product_id: u16,
    flags: u32,
}

#[repr(C)]
struct RawDevice {
    entry: DeviceEntry,
    bus_location: u32,
    devnum: u8,
}

/// The start of `LIBMTP_mtpdevice_t` (only these fields are read).
#[repr(C)]
struct MtpDevice {
    object_bitsize: u8,
    params: *mut c_void,
    usbinfo: *mut c_void,
    storage: *mut Storage,
    errorstack: *mut MtpError,
}

#[repr(C)]
struct Storage {
    id: u32,
    kind: u16,
    filesystem: u16,
    access: u16,
    capacity: u64,
    free_bytes: u64,
    free_objects: u64,
    description: *mut c_char,
    volume: *mut c_char,
    next: *mut Storage,
    prev: *mut Storage,
}

#[repr(C)]
struct File {
    item_id: u32,
    parent_id: u32,
    storage_id: u32,
    filename: *mut c_char,
    filesize: u64,
    /// `time_t`: 64 bits on every system Gezik is built for.
    modified: i64,
    filetype: c_int,
    next: *mut File,
}

#[repr(C)]
struct MtpError {
    number: c_int,
    text: *mut c_char,
    next: *mut MtpError,
}

type Progress = unsafe extern "C" fn(u64, u64, *const c_void) -> c_int;
/// `MTPDataGetFunc` and `MTPDataPutFunc`: (params, priv, length, data, done).
type Data = unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut u8, *mut u32) -> u16;

/// libmtp's functions Gezik calls.
struct Lib {
    init: unsafe extern "C" fn(),
    detect: unsafe extern "C" fn(*mut *mut RawDevice, *mut c_int) -> c_int,
    open: unsafe extern "C" fn(*mut RawDevice) -> *mut MtpDevice,
    release: unsafe extern "C" fn(*mut MtpDevice),
    get_storage: unsafe extern "C" fn(*mut MtpDevice, c_int) -> c_int,
    list: unsafe extern "C" fn(*mut MtpDevice, u32, u32) -> *mut File,
    metadata: unsafe extern "C" fn(*mut MtpDevice, u32) -> *mut File,
    destroy_file: unsafe extern "C" fn(*mut File),
    read: unsafe extern "C" fn(*mut MtpDevice, u32, Data, *mut c_void, Option<Progress>, *const c_void) -> c_int,
    send: unsafe extern "C" fn(*mut MtpDevice, Data, *mut c_void, *mut File, Option<Progress>, *const c_void) -> c_int,
    create_folder: unsafe extern "C" fn(*mut MtpDevice, *mut c_char, u32, u32) -> u32,
    delete: unsafe extern "C" fn(*mut MtpDevice, u32) -> c_int,
    rename: unsafe extern "C" fn(*mut MtpDevice, *mut File, *const c_char) -> c_int,
    errors: unsafe extern "C" fn(*mut MtpDevice) -> *mut MtpError,
    clear_errors: unsafe extern "C" fn(*mut MtpDevice),
}

impl Lib {
    /// Every function as `get` finds it, or the name of the first one missing.
    ///
    /// # Safety
    /// What `get` returns for a name must be that libmtp function (or null).
    // Each pointer becomes its field's type, written once in `Lib`.
    #[allow(clippy::missing_transmute_annotations)]
    unsafe fn bind(get: impl Fn(&CStr) -> *mut c_void) -> Result<Lib, &'static str> {
        macro_rules! f {
            ($name:literal) => {{
                let name = CStr::from_bytes_with_nul(concat!($name, "\0").as_bytes()).unwrap_or_default();
                let found = get(name);
                if found.is_null() {
                    return Err($name);
                }
                // SAFETY: the caller's promise; the field's type is the header's.
                unsafe { std::mem::transmute::<*mut c_void, _>(found) }
            }};
        }
        Ok(Lib {
            init: f!("LIBMTP_Init"),
            detect: f!("LIBMTP_Detect_Raw_Devices"),
            open: f!("LIBMTP_Open_Raw_Device_Uncached"),
            release: f!("LIBMTP_Release_Device"),
            get_storage: f!("LIBMTP_Get_Storage"),
            list: f!("LIBMTP_Get_Files_And_Folders"),
            metadata: f!("LIBMTP_Get_Filemetadata"),
            destroy_file: f!("LIBMTP_destroy_file_t"),
            read: f!("LIBMTP_Get_File_To_Handler"),
            send: f!("LIBMTP_Send_File_From_Handler"),
            create_folder: f!("LIBMTP_Create_Folder"),
            delete: f!("LIBMTP_Delete_Object"),
            rename: f!("LIBMTP_Set_File_Name"),
            errors: f!("LIBMTP_Get_Errorstack"),
            clear_errors: f!("LIBMTP_Clear_Errorstack"),
        })
    }
}

/// Where libmtp is looked for, in order: the path in settings, then Homebrew's and Gezik's
/// download (macOS) or the system's (Linux, by name).
fn candidates(configured: Option<&Path>, downloaded: Option<&Path>, mac: bool) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = configured.into_iter().map(Path::to_path_buf).collect();
    if mac {
        paths.extend(HOMEBREW.iter().map(PathBuf::from));
        paths.extend(downloaded.map(Path::to_path_buf));
    } else {
        paths.push(PathBuf::from(SYSTEM));
    }
    paths
}

/// libmtp, loaded the first time it is found (and kept: a library is not unloaded).
#[cfg(unix)]
fn library() -> io::Result<&'static Lib> {
    static LOADED: Mutex<Option<&'static Lib>> = Mutex::new(None);
    let mut loaded = LOADED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(lib) = *loaded {
        return Ok(lib);
    }
    let (configured, downloaded) = super::library_paths();
    let mut unusable = None;
    for path in candidates(configured.as_deref(), downloaded.as_deref(), cfg!(target_os = "macos")) {
        match load(&path) {
            Some(Ok(lib)) => {
                let lib: &'static Lib = Box::leak(Box::new(lib));
                // SAFETY: libmtp's set-up, once, before any other call.
                unsafe { (lib.init)() };
                *loaded = Some(lib);
                return Ok(lib);
            }
            Some(Err(missing)) => {
                let text = gezik_i18n::t!(
                    "{0} cannot be used: it has no {1} (libmtp 1.1.17 or newer is needed)",
                    path.display(),
                    missing
                );
                unusable = Some(io::Error::other(text));
            }
            None => {}
        }
    }
    Err(unusable.unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, super::NoLibmtp)))
}

/// The library at `path` bound, the name of a function it lacks, or `None` if it does not load.
#[cfg(unix)]
fn load(path: &Path) -> Option<Result<Lib, &'static str>> {
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: dlopen and dlsym with NUL-ended names; each symbol has the type libmtp's header
    // gives it. A library lacking one is closed again.
    unsafe {
        let handle = libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if handle.is_null() {
            return None;
        }
        let lib = Lib::bind(|symbol| libc::dlsym(handle, symbol.as_ptr()));
        if lib.is_err() {
            libc::dlclose(handle);
        }
        Some(lib)
    }
}

/// Opens phone `id` (`usb:<bus>:<address>`), loading libmtp first.
#[cfg(unix)]
pub(super) fn open(id: &str) -> io::Result<Box<dyn Session>> {
    let (bus, address) = super::parse_usb_id(id).ok_or(io::ErrorKind::NotFound)?;
    let lib = library()?;
    // libmtp's device search is not safe to run twice at once.
    static OPENING: Mutex<()> = Mutex::new(());
    let _one = OPENING.lock().unwrap_or_else(PoisonError::into_inner);
    let (listed, device) = {
        let mut raw: *mut RawDevice = std::ptr::null_mut();
        let mut count: c_int = 0;
        // SAFETY: libmtp fills `raw` with `count` devices (malloc'd, freed here after the
        // device is opened; it keeps no pointer into them).
        unsafe {
            (lib.detect)(&mut raw, &mut count);
            let devices: &mut [RawDevice] = if raw.is_null() {
                &mut []
            } else {
                std::slice::from_raw_parts_mut(raw, usize::try_from(count).unwrap_or(0))
            };
            let found = devices.iter_mut().find(|d| d.bus_location == bus && d.devnum == address);
            let listed = found.is_some();
            let device = found.map_or(std::ptr::null_mut(), |d| (lib.open)(d));
            if !raw.is_null() {
                libc::free(raw.cast());
            }
            (listed, device)
        }
    };
    if device.is_null() {
        return Err(not_opened(listed, may_access(bus, address), cfg!(target_os = "macos")));
    }
    Ok(Box::new(Phone::new(lib, device, id.to_owned())))
}

/// Whether this user may open the device node (Linux: the package's udev rules grant it).
#[cfg(unix)]
fn may_access(bus: u32, address: u8) -> bool {
    if cfg!(target_os = "macos") {
        return true;
    }
    let Ok(node) = std::ffi::CString::new(format!("/dev/bus/usb/{bus:03}/{address:03}")) else { return true };
    // SAFETY: a NUL-ended path.
    unsafe { libc::access(node.as_ptr(), libc::R_OK | libc::W_OK) == 0 }
}

/// Why a phone did not open: no permission (Linux), another app holding it (macOS' Image
/// Capture or `ptpcamerad`, Android File Transfer, OpenMTP; on Linux a file manager), or libmtp
/// does not see a phone there.
fn not_opened(listed: bool, may_access: bool, mac: bool) -> io::Error {
    if !may_access {
        let text = gezik_i18n::t!("No permission to open the phone (libmtp's udev rules)");
        return io::Error::new(io::ErrorKind::PermissionDenied, text.into_owned());
    }
    if !listed {
        return io::ErrorKind::NotFound.into();
    }
    let text = if mac {
        gezik_i18n::t!("Another app is using the phone (Image Capture, Android File Transfer). Quit it and try again.")
    } else {
        gezik_i18n::t!("Another app is using the phone. Quit it and try again.")
    };
    io::Error::new(io::ErrorKind::ResourceBusy, text.into_owned())
}

/// The device pointer; libmtp's calls on it are made one at a time (under the session's lock).
struct Handle(*mut MtpDevice);

// SAFETY: libmtp's device may be used from any thread, one call at a time (the Mutex).
unsafe impl Send for Handle {}

/// An open phone.
struct Phone {
    lib: &'static Lib,
    device: Mutex<Handle>,
    id: String,
    writable: bool,
}

impl Phone {
    fn new(lib: &'static Lib, device: *mut MtpDevice, id: String) -> Phone {
        let mut phone = Phone { lib, device: Mutex::new(Handle(device)), id, writable: false };
        phone.writable = phone.storages().is_ok() && phone.any_writable();
        phone
    }

    fn lock(&self) -> MutexGuard<'_, Handle> {
        self.device.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether a storage read last can be written (access capability 0: read-write).
    fn any_writable(&self) -> bool {
        let device = self.lock();
        // SAFETY: libmtp's storage list, rebuilt only by `get_storage` (under this lock).
        let mut storage = unsafe { (*device.0).storage };
        // SAFETY: as above; each entry points to the next or is null.
        while let Some(s) = unsafe { storage.as_ref() } {
            if s.access == 0 {
                return true;
            }
            storage = s.next;
        }
        false
    }

    /// The last error libmtp noted (its stack then emptied), or `fallback`.
    fn error(&self, device: &Handle, fallback: io::ErrorKind) -> io::Error {
        // SAFETY: the stack is libmtp's, read before it is cleared.
        unsafe {
            let mut text = None;
            let mut error = (self.lib.errors)(device.0);
            while let Some(e) = error.as_ref() {
                if !e.text.is_null() {
                    text = Some(CStr::from_ptr(e.text).to_string_lossy().trim().to_owned());
                }
                error = e.next;
            }
            (self.lib.clear_errors)(device.0);
            match text {
                Some(text) if !text.is_empty() => io::Error::new(fallback, text),
                _ => fallback.into(),
            }
        }
    }

    fn clear(&self, device: &Handle) {
        // SAFETY: the device is open.
        unsafe { (self.lib.clear_errors)(device.0) };
    }

    /// Deletes `item` in `storage`, what a folder holds first if the phone will not delete a
    /// folder with things in it.
    fn delete_in(&self, device: &Handle, storage: u32, item: u32) -> io::Result<()> {
        self.clear(device);
        // SAFETY: the device is open.
        if unsafe { (self.lib.delete)(device.0, item) } == 0 {
            return Ok(());
        }
        let failed = self.error(device, io::ErrorKind::Other);
        let children = self.objects(device, storage, item).map_err(|_| failed)?;
        for child in &children {
            self.delete_in(device, storage, child.item_id)?;
        }
        // SAFETY: the device is open.
        match unsafe { (self.lib.delete)(device.0, item) } {
            0 => Ok(()),
            _ => Err(self.error(device, io::ErrorKind::Other)),
        }
    }

    /// What folder `parent` of `storage` holds, as libmtp lists it.
    fn objects(&self, device: &Handle, storage: u32, parent: u32) -> io::Result<Vec<Listed>> {
        self.clear(device);
        let mut listed = Vec::new();
        // SAFETY: a list libmtp made; each one is read, then freed.
        unsafe {
            let mut file = (self.lib.list)(device.0, storage, parent);
            if file.is_null() && !(self.lib.errors)(device.0).is_null() {
                return Err(self.error(device, io::ErrorKind::Other));
            }
            while let Some(f) = file.as_ref() {
                let name = if f.filename.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(f.filename).to_string_lossy().into_owned()
                };
                listed.push(Listed {
                    item_id: f.item_id,
                    storage_id: f.storage_id,
                    name,
                    is_dir: f.filetype == FOLDER,
                    size: f.filesize,
                    modified: f.modified,
                });
                let next = f.next;
                (self.lib.destroy_file)(file);
                file = next;
            }
        }
        Ok(listed)
    }
}

/// An object as libmtp listed it.
struct Listed {
    item_id: u32,
    storage_id: u32,
    name: String,
    is_dir: bool,
    size: u64,
    modified: i64,
}

impl Drop for Phone {
    fn drop(&mut self) {
        let device = self.lock();
        // SAFETY: opened by `open`, released once.
        unsafe { (self.lib.release)(device.0) };
    }
}

/// `<storage>:<object>` of an object id.
fn parse_object(id: &str) -> io::Result<(u32, u32)> {
    let parsed = id.split_once(':').and_then(|(s, o)| Some((s.parse().ok()?, o.parse().ok()?)));
    parsed.ok_or_else(|| io::ErrorKind::InvalidInput.into())
}

fn object_id(storage: u32, item: u32) -> String {
    format!("{storage}:{item}")
}

/// `name` as a C string the call may change (libmtp may strip characters in place).
fn c_name(name: &str) -> io::Result<Vec<u8>> {
    if name.contains('\0') {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut bytes = name.as_bytes().to_vec();
    bytes.push(0);
    Ok(bytes)
}

fn date(seconds: i64) -> Option<SystemTime> {
    let seconds = u64::try_from(seconds).ok().filter(|s| *s > 0)?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

/// What a read hands each piece to.
struct Reading<'a> {
    out: &'a mut dyn Write,
    more: &'a mut dyn FnMut(u64) -> bool,
    failed: Option<io::Error>,
    stopped: bool,
}

/// What a write takes each piece from.
struct Writing<'a> {
    from: &'a mut dyn Read,
    more: &'a mut dyn FnMut(u64) -> bool,
    failed: Option<io::Error>,
    stopped: bool,
}

/// libmtp's put function: `length` bytes at `data` arrived.
unsafe extern "C" fn put_piece(_: *mut c_void, reading: *mut c_void, length: u32, data: *mut u8, done: *mut u32) -> u16 {
    // SAFETY: `reading` is the `Reading` `read_to` passed; `data` holds `length` bytes.
    let (reading, piece) = unsafe {
        (&mut *reading.cast::<Reading<'_>>(), std::slice::from_raw_parts(data, length as usize))
    };
    if let Err(err) = reading.out.write_all(piece) {
        reading.failed = Some(err);
        return HANDLER_ERROR;
    }
    // SAFETY: libmtp's out value.
    unsafe { *done = length };
    if !(reading.more)(u64::from(length)) {
        reading.stopped = true;
        return HANDLER_CANCEL;
    }
    HANDLER_OK
}

/// libmtp's get function: up to `length` bytes wanted at `data`.
unsafe extern "C" fn get_piece(_: *mut c_void, writing: *mut c_void, length: u32, data: *mut u8, done: *mut u32) -> u16 {
    // SAFETY: `writing` is the `Writing` `put` passed; `data` has room for `length` bytes.
    let (writing, room) = unsafe {
        (&mut *writing.cast::<Writing<'_>>(), std::slice::from_raw_parts_mut(data, length as usize))
    };
    let mut filled = 0;
    while filled < room.len() {
        match writing.from.read(&mut room[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => {
                writing.failed = Some(err);
                return HANDLER_ERROR;
            }
        }
    }
    // SAFETY: libmtp's out value; `filled` ≤ `length`.
    unsafe { *done = filled as u32 };
    if !(writing.more)(filled as u64) {
        writing.stopped = true;
        return HANDLER_CANCEL;
    }
    HANDLER_OK
}

impl Session for Phone {
    fn storages(&self) -> io::Result<Vec<Object>> {
        let device = self.lock();
        self.clear(&device);
        // SAFETY: the device is open; -1 also means no storage (a locked phone).
        if unsafe { (self.lib.get_storage)(device.0, 0) } != 0 {
            drop(device);
            return if self.connected() { Ok(Vec::new()) } else { Err(io::ErrorKind::NotConnected.into()) };
        }
        let mut storages = Vec::new();
        // SAFETY: the list `get_storage` just made, read under the lock.
        unsafe {
            let mut storage = (*device.0).storage;
            while let Some(s) = storage.as_ref() {
                let text = |p: *mut c_char| {
                    (!p.is_null()).then(|| CStr::from_ptr(p).to_string_lossy().trim().to_owned()).filter(|t| !t.is_empty())
                };
                let name = text(s.description).or_else(|| text(s.volume)).unwrap_or_else(|| gezik_i18n::t!("Storage {}", s.id));
                storages.push(Object { id: object_id(s.id, ROOT), name, is_dir: true, size: 0, modified: None });
                storage = s.next;
            }
        }
        Ok(storages)
    }

    fn list(&self, parent: &str) -> io::Result<Vec<Object>> {
        let (storage, folder) = parse_object(parent)?;
        let device = self.lock();
        let listed = self.objects(&device, storage, folder)?;
        Ok(listed
            .into_iter()
            .map(|l| Object {
                id: object_id(l.storage_id, l.item_id),
                name: l.name,
                is_dir: l.is_dir,
                size: if l.is_dir { 0 } else { l.size },
                modified: date(l.modified),
            })
            .collect())
    }

    fn read_to(&self, id: &str, out: &mut dyn Write, more: &mut dyn FnMut(u64) -> bool) -> io::Result<()> {
        let (_, item) = parse_object(id)?;
        let device = self.lock();
        self.clear(&device);
        let mut reading = Reading { out, more, failed: None, stopped: false };
        // SAFETY: `reading` outlives the call, which hands it to `put_piece` only.
        let result = unsafe {
            let reading = (&raw mut reading).cast::<c_void>();
            (self.lib.read)(device.0, item, put_piece, reading, None, std::ptr::null())
        };
        match (result, reading.stopped, reading.failed) {
            (0, false, None) => Ok(()),
            (_, true, _) => {
                self.clear(&device);
                Err(io::ErrorKind::Interrupted.into())
            }
            (_, _, Some(err)) => {
                self.clear(&device);
                Err(err)
            }
            _ => Err(self.error(&device, io::ErrorKind::Other)),
        }
    }

    fn put(
        &self,
        parent: &str,
        name: &str,
        size: u64,
        from: &mut dyn Read,
        more: &mut dyn FnMut(u64) -> bool,
    ) -> io::Result<Object> {
        let (storage, folder) = parse_object(parent)?;
        let mut c_name = c_name(name)?;
        let device = self.lock();
        self.clear(&device);
        // Gezik's own record: libmtp fills in the new object's id and never frees it.
        let mut file = File {
            item_id: 0,
            parent_id: folder,
            storage_id: storage,
            filename: c_name.as_mut_ptr().cast(),
            filesize: size,
            modified: 0,
            filetype: UNKNOWN,
            next: std::ptr::null_mut(),
        };
        let mut writing = Writing { from, more, failed: None, stopped: false };
        // SAFETY: `file`, its name and `writing` outlive the call, which hands `writing` to
        // `get_piece` only.
        let result = unsafe {
            let writing = (&raw mut writing).cast::<c_void>();
            (self.lib.send)(device.0, get_piece, writing, &mut file, None, std::ptr::null())
        };
        if result == 0 {
            return Ok(Object {
                id: object_id(file.storage_id, file.item_id),
                name: name.to_owned(),
                is_dir: false,
                size,
                modified: None,
            });
        }
        let err = match (writing.stopped, writing.failed) {
            (true, _) => io::ErrorKind::Interrupted.into(),
            (_, Some(err)) => err,
            _ => self.error(&device, io::ErrorKind::Other),
        };
        // The phone made the object before the data failed: nothing half written stays.
        if file.item_id != 0 {
            // SAFETY: the device is open.
            unsafe { (self.lib.delete)(device.0, file.item_id) };
        }
        self.clear(&device);
        Err(err)
    }

    fn mkdir(&self, parent: &str, name: &str) -> io::Result<Object> {
        let (storage, folder) = parse_object(parent)?;
        let mut c_name = c_name(name)?;
        let device = self.lock();
        self.clear(&device);
        // SAFETY: the name buffer outlives the call.
        let made = unsafe { (self.lib.create_folder)(device.0, c_name.as_mut_ptr().cast(), folder, storage) };
        if made == 0 {
            return Err(self.error(&device, io::ErrorKind::Other));
        }
        Ok(Object { id: object_id(storage, made), name: name.to_owned(), is_dir: true, size: 0, modified: None })
    }

    fn delete(&self, id: &str) -> io::Result<()> {
        let (storage, item) = parse_object(id)?;
        let device = self.lock();
        self.delete_in(&device, storage, item)
    }

    fn rename(&self, id: &str, name: &str) -> io::Result<()> {
        let (_, item) = parse_object(id)?;
        let c_name = c_name(name)?;
        let device = self.lock();
        self.clear(&device);
        // SAFETY: libmtp's own record of the object (its name is swapped by libmtp), freed here.
        unsafe {
            let file = (self.lib.metadata)(device.0, item);
            if file.is_null() {
                return Err(self.error(&device, io::ErrorKind::NotFound));
            }
            let renamed = (self.lib.rename)(device.0, file, c_name.as_ptr().cast());
            (self.lib.destroy_file)(file);
            if renamed != 0 {
                return Err(self.error(&device, io::ErrorKind::Other));
            }
        }
        Ok(())
    }

    fn writable(&self) -> bool {
        self.writable
    }

    fn connected(&self) -> bool {
        super::present(&self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn homebrew_comes_before_the_download_and_settings_before_both() {
        let mine = Path::new("/Users/me/lib/libmtp.9.dylib");
        let downloaded = Path::new("/Users/me/Library/Application Support/Gezik/tools/libmtp-1.1.23/libmtp.9.dylib");
        let mac = candidates(Some(mine), Some(downloaded), true);
        let expected: Vec<PathBuf> =
            [mine, Path::new(HOMEBREW[0]), Path::new(HOMEBREW[1]), downloaded].map(Path::to_path_buf).to_vec();
        assert_eq!(mac, expected);
        assert_eq!(candidates(None, None, true), HOMEBREW.map(PathBuf::from));
        // Linux: the system's by name; Gezik downloads none there.
        assert_eq!(candidates(None, Some(downloaded), false), [PathBuf::from("libmtp.so.9")]);
        assert_eq!(candidates(Some(mine), None, false), [mine.to_path_buf(), PathBuf::from("libmtp.so.9")]);
    }

    /// A stand-in for any libmtp function (never called).
    extern "C" fn nothing() {}

    #[test]
    fn a_missing_function_is_named() {
        let all = |_: &CStr| nothing as *mut c_void;
        // SAFETY: the functions are never called.
        assert!(unsafe { Lib::bind(all) }.is_ok());
        let old = |name: &CStr| if name == c"LIBMTP_Open_Raw_Device_Uncached" { std::ptr::null_mut() } else { all(name) };
        // SAFETY: as above.
        assert_eq!(unsafe { Lib::bind(old) }.err(), Some("LIBMTP_Open_Raw_Device_Uncached"));
    }

    #[test]
    fn why_a_phone_did_not_open() {
        assert_eq!(not_opened(true, false, false).kind(), io::ErrorKind::PermissionDenied);
        assert!(not_opened(true, false, false).to_string().contains("udev"));
        assert_eq!(not_opened(false, true, false).kind(), io::ErrorKind::NotFound);
        let busy = not_opened(true, true, true);
        assert_eq!(busy.kind(), io::ErrorKind::ResourceBusy);
        assert!(busy.to_string().contains("Image Capture"), "{busy}");
        assert!(!not_opened(true, true, false).to_string().contains("Image Capture"));
    }

    #[test]
    fn ids_names_and_dates() {
        assert_eq!(parse_object(&object_id(65537, ROOT)).unwrap(), (65537, ROOT));
        assert_eq!(parse_object("65537").unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert_eq!(c_name("a.jpg").unwrap(), b"a.jpg\0");
        assert!(c_name("a\0b").is_err());
        assert_eq!(date(0), None);
        assert_eq!(date(-5), None);
        assert_eq!(date(60), SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(60)));
    }

    // A fake libmtp: one storage holding a folder and a 6-byte file, sends that fail half way.

    thread_local! {
        static DELETED: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
        static ERRORS: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    fn file(item: u32, name: &'static CStr, folder: bool, next: *mut File) -> *mut File {
        Box::into_raw(Box::new(File {
            item_id: item,
            parent_id: ROOT,
            storage_id: 7,
            filename: name.as_ptr().cast_mut(),
            filesize: if folder { 4096 } else { 6 },
            modified: 1_700_000_000,
            filetype: if folder { FOLDER } else { 5 },
            next,
        }))
    }

    unsafe extern "C" fn fake_list(_: *mut MtpDevice, storage: u32, parent: u32) -> *mut File {
        if storage == 7 && parent == ROOT { file(10, c"DCIM", true, file(11, c"a.txt", false, std::ptr::null_mut())) } else { std::ptr::null_mut() }
    }

    unsafe extern "C" fn fake_destroy(file: *mut File) {
        // SAFETY: made by `file`.
        drop(unsafe { Box::from_raw(file) });
    }

    unsafe extern "C" fn fake_read(
        device: *mut MtpDevice,
        _: u32,
        put: Data,
        reading: *mut c_void,
        _: Option<Progress>,
        _: *const c_void,
    ) -> c_int {
        for mut piece in [*b"abc", *b"def"] {
            let mut done = 0;
            // SAFETY: as libmtp calls it.
            if unsafe { put(device.cast(), reading, 3, piece.as_mut_ptr(), &mut done) } != HANDLER_OK || done != 3 {
                return -1;
            }
        }
        0
    }

    /// Makes object 99, takes 4 bytes, then fails (as an unplugged phone).
    unsafe extern "C" fn fake_send(
        device: *mut MtpDevice,
        get: Data,
        writing: *mut c_void,
        file: *mut File,
        _: Option<Progress>,
        _: *const c_void,
    ) -> c_int {
        // SAFETY: as libmtp does.
        unsafe {
            (*file).item_id = 99;
            let mut room = [0u8; 4];
            let mut done = 0;
            if get(device.cast(), writing, 4, room.as_mut_ptr(), &mut done) != HANDLER_OK {
                return -1;
            }
            ERRORS.with(|e| *e.borrow_mut() = b"PTP Layer error 02ff: Could not send object.\0".to_vec());
        }
        -1
    }

    unsafe extern "C" fn fake_delete(_: *mut MtpDevice, item: u32) -> c_int {
        DELETED.with(|d| d.borrow_mut().push(item));
        0
    }

    unsafe extern "C" fn fake_errors(_: *mut MtpDevice) -> *mut MtpError {
        thread_local!(static ERROR: RefCell<MtpError> = const {
            RefCell::new(MtpError { number: 1, text: std::ptr::null_mut(), next: std::ptr::null_mut() })
        });
        ERRORS.with(|e| {
            let mut text = e.borrow_mut();
            if text.is_empty() {
                return std::ptr::null_mut();
            }
            ERROR.with(|error| {
                error.borrow_mut().text = text.as_mut_ptr().cast();
                error.as_ptr()
            })
        })
    }

    unsafe extern "C" fn fake_clear(_: *mut MtpDevice) {
        ERRORS.with(|e| e.borrow_mut().clear());
    }

    unsafe extern "C" fn fake_release(_: *mut MtpDevice) {}

    fn fake_phone() -> Phone {
        let fake = |name: &CStr| -> *mut c_void {
            match name.to_bytes() {
                b"LIBMTP_Get_Files_And_Folders" => fake_list as *mut c_void,
                b"LIBMTP_destroy_file_t" => fake_destroy as *mut c_void,
                b"LIBMTP_Get_File_To_Handler" => fake_read as *mut c_void,
                b"LIBMTP_Send_File_From_Handler" => fake_send as *mut c_void,
                b"LIBMTP_Delete_Object" => fake_delete as *mut c_void,
                b"LIBMTP_Get_Errorstack" => fake_errors as *mut c_void,
                b"LIBMTP_Clear_Errorstack" => fake_clear as *mut c_void,
                b"LIBMTP_Release_Device" => fake_release as *mut c_void,
                _ => nothing as *mut c_void,
            }
        };
        // SAFETY: only the functions above are called.
        let lib: &'static Lib = Box::leak(Box::new(unsafe { Lib::bind(fake) }.unwrap()));
        let device = Box::leak(Box::new(MtpDevice {
            object_bitsize: 32,
            params: std::ptr::null_mut(),
            usbinfo: std::ptr::null_mut(),
            storage: std::ptr::null_mut(),
            errorstack: std::ptr::null_mut(),
        }));
        Phone { lib, device: Mutex::new(Handle(device)), id: "usb:1:7".into(), writable: true }
    }

    #[test]
    fn a_folder_is_listed_with_ids_kinds_and_dates() {
        let phone = fake_phone();
        let listed = phone.list(&object_id(7, ROOT)).unwrap();
        let rows: Vec<(&str, &str, bool, u64)> =
            listed.iter().map(|o| (o.id.as_str(), o.name.as_str(), o.is_dir, o.size)).collect();
        assert_eq!(rows, [("7:10", "DCIM", true, 0), ("7:11", "a.txt", false, 6)]);
        assert_eq!(listed[1].modified, date(1_700_000_000));
        assert!(phone.list("7:10").unwrap().is_empty(), "an empty folder (no error noted)");
    }

    #[test]
    fn a_read_arrives_whole_or_stops_when_told() {
        let phone = fake_phone();
        let mut out = Vec::new();
        let mut counted = 0;
        phone.read_to("7:11", &mut out, &mut |n| {
            counted += n;
            true
        })
        .unwrap();
        assert_eq!((out.as_slice(), counted), (&b"abcdef"[..], 6));
        let mut out = Vec::new();
        let err = phone.read_to("7:11", &mut out, &mut |_| false).unwrap_err();
        assert_eq!((err.kind(), out.as_slice()), (io::ErrorKind::Interrupted, &b"abc"[..]));
    }

    #[test]
    fn a_failed_write_leaves_nothing_on_the_phone() {
        let phone = fake_phone();
        DELETED.with(|d| d.borrow_mut().clear());
        let err = phone.put("7:10", "b.txt", 6, &mut &b"abcdef"[..], &mut |_| true).unwrap_err();
        assert!(err.to_string().contains("Could not send object"), "{err}");
        assert_eq!(DELETED.with(|d| d.borrow().clone()), [99]);
        // Stopped: the half-made object goes too, and the error says stopped.
        let err = phone.put("7:10", "b.txt", 6, &mut &b"abcdef"[..], &mut |_| false).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert_eq!(DELETED.with(|d| d.borrow().clone()), [99, 99]);
        assert!(ERRORS.with(|e| e.borrow().is_empty()), "the error stack is emptied");
    }
}

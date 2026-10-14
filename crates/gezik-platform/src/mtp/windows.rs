//! Windows: phones through WPD (spec 16 §10.2). Finding them reads only the device interface
//! list (cfgmgr32, loaded already); WPD's COM objects are made when a phone is opened. They are
//! the free-threaded kind (`PortableDeviceFTM`), so any worker may call them; the session keeps
//! the process's multithreaded apartment alive while it lives.

use std::io;
use std::time::{Duration, SystemTime};

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_DevNode_PropertyW, CM_Get_Device_Interface_List_SizeW,
    CM_Get_Device_Interface_ListW, CM_Get_Device_Interface_PropertyW, CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW,
    CR_BUFFER_SMALL, CR_SUCCESS,
};
use windows::Win32::Devices::PortableDevices::{
    GUID_DEVINTERFACE_WPD, IPortableDevice, IPortableDeviceContent, IPortableDeviceDataStream,
    IPortableDeviceKeyCollection, IPortableDevicePropVariantCollection, IPortableDeviceProperties,
    IPortableDeviceResources, IPortableDeviceValues, PORTABLE_DEVICE_DELETE_NO_RECURSION,
    PORTABLE_DEVICE_DELETE_WITH_RECURSION, PortableDeviceFTM, PortableDeviceKeyCollection,
    PortableDevicePropVariantCollection, PortableDeviceValues, WPD_CLIENT_DESIRED_ACCESS, WPD_CLIENT_MAJOR_VERSION,
    WPD_CLIENT_MINOR_VERSION, WPD_CLIENT_NAME, WPD_CLIENT_REVISION, WPD_CONTENT_TYPE_FOLDER,
    WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT, WPD_CONTENT_TYPE_GENERIC_FILE, WPD_DEVICE_OBJECT_ID,
    WPD_FUNCTIONAL_CATEGORY_STORAGE, WPD_FUNCTIONAL_OBJECT_CATEGORY, WPD_OBJECT_CONTENT_TYPE, WPD_OBJECT_DATE_MODIFIED,
    WPD_OBJECT_FORMAT, WPD_OBJECT_FORMAT_PROPERTIES_ONLY, WPD_OBJECT_FORMAT_UNSPECIFIED, WPD_OBJECT_NAME,
    WPD_OBJECT_ORIGINAL_FILE_NAME, WPD_OBJECT_PARENT_ID, WPD_OBJECT_SIZE, WPD_PROPERTY_ATTRIBUTE_CAN_WRITE,
    WPD_RESOURCE_DEFAULT, WPD_STORAGE_ACCESS_CAPABILITY,
};
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_DeviceDesc, DEVPKEY_Device_FriendlyName, DEVPKEY_Device_InstanceId,
    DEVPKEY_DeviceInterface_FriendlyName, DEVPROPTYPE,
};
use windows::Win32::Foundation::{DEVPROPKEY, E_ACCESSDENIED, GENERIC_READ, GENERIC_WRITE, PROPERTYKEY};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CO_MTA_USAGE_COOKIE, CoCreateInstance, CoDecrementMTAUsage, CoIncrementMTAUsage,
    CoTaskMemFree, IStream, STGC_DEFAULT, STGM_READ,
};
use windows::Win32::System::Variant::{VT_DATE, VT_LPWSTR};
use windows::core::{GUID, Interface, PCWSTR, PWSTR};

use super::{Object, Session};

/// The WPD interfaces present now: (interface path, name).
pub(super) fn interfaces() -> Vec<(String, String)> {
    // The list may grow between asking its size and reading it: try again.
    for _ in 0..3 {
        let mut len = 0u32;
        let size = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &mut len,
                &GUID_DEVINTERFACE_WPD,
                PCWSTR::null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if size != CR_SUCCESS || len <= 1 {
            return Vec::new();
        }
        let mut buffer = vec![0u16; len as usize];
        let read = unsafe {
            CM_Get_Device_Interface_ListW(
                &GUID_DEVINTERFACE_WPD,
                PCWSTR::null(),
                &mut buffer,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if read == CR_BUFFER_SMALL {
            continue;
        }
        if read != CR_SUCCESS {
            return Vec::new();
        }
        return buffer
            .split(|&c| c == 0)
            .filter(|id| !id.is_empty())
            .map(|id| {
                let id = String::from_utf16_lossy(id);
                let name = name_of(&id);
                (id, name)
            })
            .collect();
    }
    Vec::new()
}

/// The device's name: the interface's own, else its device's (as Explorer shows it).
fn name_of(id: &str) -> String {
    let wide = wide(id);
    let interface = |key: &DEVPROPKEY| {
        property(|ty, buffer, size| unsafe {
            CM_Get_Device_Interface_PropertyW(PCWSTR(wide.as_ptr()), key, ty, buffer, size, 0)
        })
    };
    if let Some(name) = interface(&DEVPKEY_DeviceInterface_FriendlyName) {
        return name;
    }
    let Some(instance) = interface(&DEVPKEY_Device_InstanceId) else { return String::new() };
    let instance = self::wide(&instance);
    let mut node = 0u32;
    if unsafe { CM_Locate_DevNodeW(&mut node, PCWSTR(instance.as_ptr()), CM_LOCATE_DEVNODE_NORMAL) } != CR_SUCCESS {
        return String::new();
    }
    let device = |key: &DEVPROPKEY| {
        property(|ty, buffer, size| unsafe { CM_Get_DevNode_PropertyW(node, key, ty, buffer, size, 0) })
    };
    device(&DEVPKEY_Device_FriendlyName).or_else(|| device(&DEVPKEY_Device_DeviceDesc)).unwrap_or_default()
}

/// A text property read by `get` (its size first, then the bytes).
fn property(
    get: impl Fn(
        *mut DEVPROPTYPE,
        Option<*mut u8>,
        *mut u32,
    ) -> windows::Win32::Devices::DeviceAndDriverInstallation::CONFIGRET,
) -> Option<String> {
    let mut ty = DEVPROPTYPE::default();
    let mut size = 0u32;
    let _ = get(&mut ty, None, &mut size);
    if size < 2 {
        return None;
    }
    let mut buffer = vec![0u16; size as usize / 2];
    if get(&mut ty, Some(buffer.as_mut_ptr().cast()), &mut size) != CR_SUCCESS {
        return None;
    }
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..end]);
    (!text.trim().is_empty()).then_some(text)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Opens the device `id`, for reading and writing if it lets Gezik, else for reading.
pub(super) fn open(id: &str) -> io::Result<Box<dyn Session>> {
    let cookie = unsafe { CoIncrementMTAUsage() }?;
    let opened = Com::open(id, true).or_else(|err| match err.code() {
        E_ACCESSDENIED => Com::open(id, false),
        _ => Err(err),
    });
    match opened {
        Ok(com) => Ok(Box::new(Wpd { id: id.to_owned(), com: Some(com), cookie })),
        Err(err) => {
            let _ = unsafe { CoDecrementMTAUsage(cookie) };
            Err(err.into())
        }
    }
}

struct Com {
    device: IPortableDevice,
    content: IPortableDeviceContent,
    properties: IPortableDeviceProperties,
    resources: IPortableDeviceResources,
    /// The properties read for a row.
    keys: IPortableDeviceKeyCollection,
    write: bool,
}

impl Com {
    fn open(id: &str, write: bool) -> windows::core::Result<Com> {
        unsafe {
            let device: IPortableDevice = CoCreateInstance(&PortableDeviceFTM, None, CLSCTX_INPROC_SERVER)?;
            let client: IPortableDeviceValues = CoCreateInstance(&PortableDeviceValues, None, CLSCTX_INPROC_SERVER)?;
            client.SetStringValue(&WPD_CLIENT_NAME, windows::core::w!("Gezik"))?;
            client.SetUnsignedIntegerValue(&WPD_CLIENT_MAJOR_VERSION, 1)?;
            client.SetUnsignedIntegerValue(&WPD_CLIENT_MINOR_VERSION, 0)?;
            client.SetUnsignedIntegerValue(&WPD_CLIENT_REVISION, 0)?;
            let access = if write { GENERIC_READ.0 | GENERIC_WRITE.0 } else { GENERIC_READ.0 };
            client.SetUnsignedIntegerValue(&WPD_CLIENT_DESIRED_ACCESS, access)?;
            let wide = wide(id);
            device.Open(PCWSTR(wide.as_ptr()), &client)?;
            let content = device.Content()?;
            let properties = content.Properties()?;
            let resources = content.Transfer()?;
            let keys: IPortableDeviceKeyCollection =
                CoCreateInstance(&PortableDeviceKeyCollection, None, CLSCTX_INPROC_SERVER)?;
            for key in [
                &WPD_OBJECT_ORIGINAL_FILE_NAME,
                &WPD_OBJECT_NAME,
                &WPD_OBJECT_CONTENT_TYPE,
                &WPD_OBJECT_SIZE,
                &WPD_OBJECT_DATE_MODIFIED,
                &WPD_FUNCTIONAL_OBJECT_CATEGORY,
                &WPD_STORAGE_ACCESS_CAPABILITY,
            ] {
                keys.Add(key)?;
            }
            Ok(Com { device, content, properties, resources, keys, write })
        }
    }
}

struct Wpd {
    id: String,
    /// Taken (and the device closed) before the apartment is let go of.
    com: Option<Com>,
    cookie: CO_MTA_USAGE_COOKIE,
}

// SAFETY: WPD's free-threaded objects (`PortableDeviceFTM`) may be called from any thread.
unsafe impl Send for Wpd {}
unsafe impl Sync for Wpd {}

impl Drop for Wpd {
    fn drop(&mut self) {
        if let Some(com) = self.com.take() {
            let _ = unsafe { com.device.Close() };
        }
        let _ = unsafe { CoDecrementMTAUsage(self.cookie) };
    }
}

/// A row's facts as read, with what kind of object it is.
struct Read {
    object: Object,
    kind: GUID,
    category: GUID,
    /// A storage's access (0: read and write).
    access: u32,
}

impl Wpd {
    fn com(&self) -> &Com {
        self.com.as_ref().expect("open while the session lives")
    }

    /// The ids `parent` holds.
    fn children(&self, parent: &str) -> io::Result<Vec<String>> {
        let parent = wide(parent);
        let found = unsafe { self.com().content.EnumObjects(0, PCWSTR(parent.as_ptr()), None)? };
        let mut ids = Vec::new();
        loop {
            let mut batch = [PWSTR::null(); 64];
            let mut got = 0u32;
            let result = unsafe { found.Next(&mut batch, &mut got) };
            for id in &batch[..got as usize] {
                ids.push(unsafe { id.to_string() }.unwrap_or_default());
                unsafe { CoTaskMemFree(Some(id.0 as *const _)) };
            }
            result.ok()?;
            // S_FALSE: fewer than asked, the last batch.
            if got < batch.len() as u32 || result != windows::Win32::Foundation::S_OK {
                return Ok(ids);
            }
        }
    }

    fn read(&self, id: &str) -> io::Result<Read> {
        let wide = wide(id);
        let values = unsafe { self.com().properties.GetValues(PCWSTR(wide.as_ptr()), &self.com().keys)? };
        let text = |key: &PROPERTYKEY| unsafe {
            let value = values.GetStringValue(key).ok()?;
            let text = value.to_string().ok();
            CoTaskMemFree(Some(value.0 as *const _));
            text.filter(|text| !text.is_empty())
        };
        let kind = unsafe { values.GetGuidValue(&WPD_OBJECT_CONTENT_TYPE) }.unwrap_or_default();
        let name = text(&WPD_OBJECT_ORIGINAL_FILE_NAME).or_else(|| text(&WPD_OBJECT_NAME)).unwrap_or_else(|| id.into());
        let object = Object {
            id: id.to_owned(),
            name,
            is_dir: kind == WPD_CONTENT_TYPE_FOLDER || kind == WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT,
            size: unsafe { values.GetUnsignedLargeIntegerValue(&WPD_OBJECT_SIZE) }.unwrap_or(0),
            modified: unsafe { values.GetValue(&WPD_OBJECT_DATE_MODIFIED) }.ok().and_then(date),
        };
        Ok(Read {
            object,
            kind,
            category: unsafe { values.GetGuidValue(&WPD_FUNCTIONAL_OBJECT_CATEGORY) }.unwrap_or_default(),
            access: unsafe { values.GetUnsignedIntegerValue(&WPD_STORAGE_ACCESS_CAPABILITY) }.unwrap_or(0),
        })
    }

    fn storage_rows(&self) -> io::Result<Vec<Read>> {
        let device = unsafe { WPD_DEVICE_OBJECT_ID.to_string() }.unwrap_or_else(|_| "DEVICE".into());
        let mut storages = Vec::new();
        for id in self.children(&device)? {
            let read = self.read(&id)?;
            if read.kind == WPD_CONTENT_TYPE_FUNCTIONAL_OBJECT && read.category == WPD_FUNCTIONAL_CATEGORY_STORAGE {
                storages.push(read);
            }
        }
        Ok(storages)
    }

    fn new_values(&self, parent: &str, name: &str) -> io::Result<IPortableDeviceValues> {
        unsafe {
            let values: IPortableDeviceValues = CoCreateInstance(&PortableDeviceValues, None, CLSCTX_INPROC_SERVER)?;
            let (parent, name) = (wide(parent), wide(name));
            values.SetStringValue(&WPD_OBJECT_PARENT_ID, PCWSTR(parent.as_ptr()))?;
            values.SetStringValue(&WPD_OBJECT_ORIGINAL_FILE_NAME, PCWSTR(name.as_ptr()))?;
            values.SetStringValue(&WPD_OBJECT_NAME, PCWSTR(name.as_ptr()))?;
            Ok(values)
        }
    }

    fn delete_one(&self, id: &str, recursion: bool) -> io::Result<()> {
        unsafe {
            let ids: IPortableDevicePropVariantCollection =
                CoCreateInstance(&PortableDevicePropVariantCollection, None, CLSCTX_INPROC_SERVER)?;
            let mut text = wide(id);
            let mut value = PROPVARIANT::default();
            // The collection copies the text; `value` is not cleared (it does not own it).
            (*value.Anonymous.Anonymous).vt = VT_LPWSTR;
            (*value.Anonymous.Anonymous).Anonymous.pwszVal = PWSTR(text.as_mut_ptr());
            ids.Add(&value)?;
            let how =
                if recursion { PORTABLE_DEVICE_DELETE_WITH_RECURSION } else { PORTABLE_DEVICE_DELETE_NO_RECURSION };
            self.com().content.Delete(how.0 as u32, &ids, std::ptr::null_mut())?;
            Ok(())
        }
    }
}

/// A `VT_DATE` (days since 30 December 1899) as a time.
// shortcut: WPD gives the device's local time; read as UTC (off by the time zone), convert if it matters.
fn date(mut value: PROPVARIANT) -> Option<SystemTime> {
    let days = unsafe {
        let inner = &value.Anonymous.Anonymous;
        (inner.vt == VT_DATE).then_some(inner.Anonymous.date)
    };
    let _ = unsafe { PropVariantClear(&mut value) };
    let seconds = (days? - 25_569.0) * 86_400.0;
    (seconds.is_finite() && seconds >= 0.0).then(|| SystemTime::UNIX_EPOCH + Duration::from_secs_f64(seconds))
}

const PIECE: usize = 256 * 1024;

impl Session for Wpd {
    fn storages(&self) -> io::Result<Vec<Object>> {
        Ok(self.storage_rows()?.into_iter().map(|read| read.object).collect())
    }

    fn list(&self, parent: &str) -> io::Result<Vec<Object>> {
        self.children(parent)?.iter().map(|id| self.read(id).map(|read| read.object)).collect()
    }

    fn read_to(&self, id: &str, out: &mut dyn io::Write, more: &mut dyn FnMut(u64) -> bool) -> io::Result<()> {
        let wide = wide(id);
        let mut optimal = 0u32;
        let mut stream: Option<IStream> = None;
        unsafe {
            self.com().resources.GetStream(
                PCWSTR(wide.as_ptr()),
                &WPD_RESOURCE_DEFAULT,
                STGM_READ.0,
                &mut optimal,
                &mut stream,
            )?;
        }
        let stream = stream.ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        let mut buffer = vec![0u8; (optimal as usize).clamp(64 * 1024, PIECE)];
        loop {
            let mut got = 0u32;
            unsafe { stream.Read(buffer.as_mut_ptr().cast(), buffer.len() as u32, Some(&mut got)) }.ok()?;
            if got == 0 {
                return Ok(());
            }
            out.write_all(&buffer[..got as usize])?;
            if !more(u64::from(got)) {
                return Err(io::ErrorKind::Interrupted.into());
            }
        }
    }

    fn put(
        &self,
        parent: &str,
        name: &str,
        size: u64,
        from: &mut dyn io::Read,
        more: &mut dyn FnMut(u64) -> bool,
    ) -> io::Result<Object> {
        let values = self.new_values(parent, name)?;
        let mut stream: Option<IStream> = None;
        let mut optimal = 0u32;
        unsafe {
            values.SetUnsignedLargeIntegerValue(&WPD_OBJECT_SIZE, size)?;
            values.SetGuidValue(&WPD_OBJECT_CONTENT_TYPE, &WPD_CONTENT_TYPE_GENERIC_FILE)?;
            values.SetGuidValue(&WPD_OBJECT_FORMAT, &WPD_OBJECT_FORMAT_UNSPECIFIED)?;
            self.com().content.CreateObjectWithPropertiesAndData(
                &values,
                &mut stream,
                &mut optimal,
                std::ptr::null_mut(),
            )?;
        }
        let stream = stream.ok_or_else(|| io::Error::from(io::ErrorKind::Other))?;
        let mut buffer = vec![0u8; (optimal as usize).clamp(64 * 1024, PIECE)];
        // Until committed nothing is made on the device: a stop or a failure reverts it.
        let written = (|| -> io::Result<()> {
            loop {
                let n = from.read(&mut buffer)?;
                if n == 0 {
                    return Ok(());
                }
                let mut done = 0u32;
                unsafe { stream.Write(buffer.as_ptr().cast(), n as u32, Some(&mut done)) }.ok()?;
                if !more(n as u64) {
                    return Err(io::ErrorKind::Interrupted.into());
                }
            }
        })();
        if let Err(err) = written {
            let _ = unsafe { stream.Revert() };
            return Err(err);
        }
        unsafe { stream.Commit(STGC_DEFAULT)? };
        let id = unsafe {
            let made = stream.cast::<IPortableDeviceDataStream>()?.GetObjectID()?;
            let id = made.to_string().unwrap_or_default();
            CoTaskMemFree(Some(made.0 as *const _));
            id
        };
        Ok(Object { id, name: name.to_owned(), is_dir: false, size, modified: Some(SystemTime::now()) })
    }

    fn mkdir(&self, parent: &str, name: &str) -> io::Result<Object> {
        let values = self.new_values(parent, name)?;
        let id = unsafe {
            values.SetGuidValue(&WPD_OBJECT_CONTENT_TYPE, &WPD_CONTENT_TYPE_FOLDER)?;
            values.SetGuidValue(&WPD_OBJECT_FORMAT, &WPD_OBJECT_FORMAT_PROPERTIES_ONLY)?;
            let mut made = PWSTR::null();
            self.com().content.CreateObjectWithPropertiesOnly(&values, &mut made)?;
            let id = made.to_string().unwrap_or_default();
            CoTaskMemFree(Some(made.0 as *const _));
            id
        };
        Ok(Object { id, name: name.to_owned(), is_dir: true, size: 0, modified: Some(SystemTime::now()) })
    }

    fn delete(&self, id: &str) -> io::Result<()> {
        if self.delete_one(id, true).is_ok() {
            return Ok(());
        }
        // Some devices delete only empty folders: what it holds first, deepest first.
        for child in self.children(id).unwrap_or_default() {
            self.delete(&child)?;
        }
        self.delete_one(id, false)
    }

    fn rename(&self, id: &str, name: &str) -> io::Result<()> {
        unsafe {
            let values: IPortableDeviceValues = CoCreateInstance(&PortableDeviceValues, None, CLSCTX_INPROC_SERVER)?;
            let name = wide(name);
            values.SetStringValue(&WPD_OBJECT_ORIGINAL_FILE_NAME, PCWSTR(name.as_ptr()))?;
            let id = wide(id);
            let results = self.com().properties.SetValues(PCWSTR(id.as_ptr()), &values)?;
            match results.GetErrorValue(&WPD_OBJECT_ORIGINAL_FILE_NAME) {
                Ok(result) if result.is_err() => Err(windows::core::Error::from(result).into()),
                _ => Ok(()),
            }
        }
    }

    fn can_rename(&self, id: &str) -> bool {
        let id = wide(id);
        unsafe {
            self.com()
                .properties
                .GetPropertyAttributes(PCWSTR(id.as_ptr()), &WPD_OBJECT_ORIGINAL_FILE_NAME)
                .and_then(|attributes| attributes.GetBoolValue(&WPD_PROPERTY_ATTRIBUTE_CAN_WRITE))
                .is_ok_and(|can| can.as_bool())
        }
    }

    fn writable(&self) -> bool {
        self.com().write && self.storage_rows().is_ok_and(|rows| rows.iter().any(|read| read.access == 0))
    }

    fn connected(&self) -> bool {
        super::present(&self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lists what WPD offers here, read only (a USB stick shows as one): run by hand.
    #[test]
    #[ignore]
    fn lists_portable_devices_read_only() {
        for (id, name) in interfaces() {
            println!("{name}: {id}");
            let Ok(session) = open(&id) else { continue };
            for storage in session.storages().unwrap_or_default() {
                println!("  {} ({})", storage.name, storage.id);
                for row in session.list(&storage.id).unwrap_or_default().iter().take(10) {
                    println!("    {} dir={} {} {:?}", row.name, row.is_dir, row.size, row.modified);
                }
            }
        }
    }
}

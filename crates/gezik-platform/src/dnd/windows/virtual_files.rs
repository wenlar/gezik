//! Windows: items another program offers with no file behind them (`FileGroupDescriptorW` and
//! `FileContents`: an Outlook attachment or message, a browser's picture, a file in a zip
//! view). The descriptor is read on the UI thread; the contents on the job's thread, through
//! the Global Interface Table: the data object stays in the UI thread's apartment, which goes
//! on pumping messages, so the calls reach the source (spec 9 §8.1). Everything the source
//! hands over (counts, names, sizes, memory, streams) is untrusted.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{E_ABORT, HGLOBAL, S_OK};
use windows::Win32::System::Com::StructuredStorage::{IStorage, STGFMT_STORAGE, StgCreateStorageEx};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, DVASPECT_CONTENT, FORMATETC,
    IDataObject, IGlobalInterfaceTable, IStream, STGC_DEFAULT, STGM_READWRITE, STGM_SHARE_EXCLUSIVE, STGMEDIUM,
    STREAM_SEEK_SET, TYMED_HGLOBAL, TYMED_ISTORAGE, TYMED_ISTREAM,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_NONE, ReleaseStgMedium};
use windows::Win32::UI::Shell::{
    CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, CFSTR_INETURLA, CFSTR_INETURLW, IDataObjectAsyncCapability,
};
use windows_core::{GUID, HSTRING, Interface, PCWSTR};

use crate::dnd::{VirtualEntry, VirtualFiles, VirtualSource};

/// `FILEDESCRIPTORW`: its size, and where its name (260 UTF-16 units, NUL-terminated) starts.
const DESCRIPTOR: usize = 592;
const NAME_AT: usize = 72;
const FD_ATTRIBUTES: u32 = 0x4;
const FD_FILESIZE: u32 = 0x40;
const DIRECTORY: u32 = 0x10;
/// The most items read from one descriptor.
const MOST: usize = 10_000;
/// How much is written between progress reports.
const CHUNK: usize = 256 * 1024;
/// `CLSID_StdGlobalInterfaceTable` (not in the `windows` crate).
const GLOBAL_TABLE: GUID = GUID::from_u128(0x00000323_0000_0000_c000_000000000046);

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at + 4).and_then(|b| b.try_into().ok()).map(u32::from_le_bytes)
}

/// The items of a `FILEGROUPDESCRIPTORW` (`cItems`, then that many `FILEDESCRIPTORW`s), as many
/// whole ones as the bytes hold, at most 10,000. A name ends at its first NUL; one with no NUL
/// in its 260 units is broken (`failed`). Making names safe is the job's
/// (`gezik_core::drop_names`).
pub(crate) fn descriptor_entries(bytes: &[u8]) -> Vec<VirtualEntry> {
    let count = u32_at(bytes, 0).map_or(0, |n| n as usize);
    (0..count.min(MOST))
        .map_while(|i| {
            let d = bytes.get(4 + i * DESCRIPTOR..4 + (i + 1) * DESCRIPTOR)?;
            let flags = u32_at(d, 0)?;
            let attributes = u32_at(d, 36)?;
            let size = (u64::from(u32_at(d, 64)?) << 32) | u64::from(u32_at(d, 68)?);
            let units: Vec<u16> =
                d.get(NAME_AT..)?.as_chunks::<2>().0.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
            let end = units.iter().position(|&unit| unit == 0);
            Some(VirtualEntry {
                name: String::from_utf16_lossy(&units[..end.unwrap_or(units.len())]),
                is_dir: flags & FD_ATTRIBUTES != 0 && attributes & DIRECTORY != 0,
                size: (flags & FD_FILESIZE != 0).then_some(size),
                failed: end.is_none().then(|| "The program gave a broken name".to_string()),
            })
        })
        .collect()
}

/// Leaves out the internet shortcut a browser adds to a dragged link or picture (`page.url`)
/// when the drag also carries the URL: a link is no file (spec 9 §17 decision 22). Each item
/// keeps its place in the source (`lindex`).
pub(crate) fn without_links(entries: Vec<VirtualEntry>, has_url: bool) -> Vec<(i32, VirtualEntry)> {
    entries
        .into_iter()
        .enumerate()
        .filter(|(_, entry)| !(has_url && is_link(&entry.name)))
        .map(|(i, entry)| (i as i32, entry))
        .collect()
}

/// Whether `name` is written as an internet shortcut: judged on the name as it would be
/// written (`x.url.` loses its dot), not as given.
fn is_link(name: &str) -> bool {
    gezik_core::drop_names::safe_relative(name, true)
        .ok()
        .and_then(|path| path.file_name().map(|last| gezik_core::drop_names::is_link_file(&last.to_string_lossy())))
        .unwrap_or(false)
}

fn format_id(name: PCWSTR) -> u16 {
    unsafe { RegisterClipboardFormatW(name) as u16 }
}

fn formatetc(id: u16, lindex: i32, tymed: u32) -> FORMATETC {
    FORMATETC { cfFormat: id, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex, tymed }
}

fn io_error(err: windows_core::Error) -> io::Error {
    io::Error::other(err.message())
}

/// The items `data` offers with no file behind them (none without a descriptor). On the UI
/// thread, when a drag comes over the window: one small block of memory.
pub(crate) fn offered(data: &IDataObject) -> Vec<(i32, VirtualEntry)> {
    let hglobal = TYMED_HGLOBAL.0 as u32;
    let bytes = unsafe {
        let Ok(mut medium) = data.GetData(&formatetc(format_id(CFSTR_FILEDESCRIPTORW), -1, hglobal)) else {
            return Vec::new();
        };
        // Never more than the most descriptors read: the size is the source's to claim.
        let most = 4 + MOST * DESCRIPTOR;
        let bytes = if medium.tymed == hglobal { global_bytes(medium.u.hGlobal, most) } else { Vec::new() };
        ReleaseStgMedium(&mut medium);
        bytes
    };
    let has_url = [CFSTR_INETURLW, CFSTR_INETURLA]
        .into_iter()
        .any(|name| unsafe { data.QueryGetData(&formatetc(format_id(name), -1, hglobal)) } == S_OK);
    without_links(descriptor_entries(&bytes), has_url)
}

/// A copy of global memory `memory`, at most `most` bytes of it.
unsafe fn global_bytes(memory: HGLOBAL, most: usize) -> Vec<u8> {
    unsafe {
        let size = GlobalSize(memory).min(most);
        let at = GlobalLock(memory);
        if at.is_null() {
            return Vec::new();
        }
        let bytes = std::slice::from_raw_parts(at.cast::<u8>(), size).to_vec();
        let _ = GlobalUnlock(memory);
        bytes
    }
}

/// The process's Global Interface Table, COM started on this thread first, once (in a
/// single-threaded apartment, as the engine's trash calls want; a thread already in one, the
/// UI thread among them, keeps its own).
fn table() -> io::Result<IGlobalInterfaceTable> {
    thread_local!(static COM: () = { let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }; });
    COM.with(|_| ());
    unsafe { CoCreateInstance(&GLOBAL_TABLE, None, CLSCTX_INPROC_SERVER) }.map_err(io_error)
}

/// This thread's handle on the data object behind `cookie`.
fn from_table(table: &IGlobalInterfaceTable, cookie: u32) -> io::Result<IDataObject> {
    let mut raw = std::ptr::null_mut();
    unsafe {
        table.GetInterfaceFromGlobal(cookie, &IDataObject::IID, &mut raw).map_err(io_error)?;
        Ok(IDataObject::from_raw(raw))
    }
}

/// A dropped data object, held for the job until it ends.
struct Source {
    cookie: u32,
    /// (`lindex`, item).
    entries: Vec<(i32, VirtualEntry)>,
    /// The source extracts asynchronously: it is told when the job is over.
    started: bool,
    wrote: AtomicBool,
}

/// Holds `data` for the job that writes its items (`None`: it offers none, or COM fails). At
/// the drop, on the UI thread.
pub(crate) fn files(data: &IDataObject) -> Option<VirtualFiles> {
    let entries = offered(data);
    if entries.is_empty() {
        return None;
    }
    let cookie = unsafe { table().ok()?.RegisterInterfaceInGlobal(data, &IDataObject::IID) }.ok()?;
    let started = match data.cast::<IDataObjectAsyncCapability>() {
        Ok(capability) if unsafe { capability.GetAsyncMode() }.is_ok_and(|on| on.as_bool()) => {
            unsafe { capability.StartOperation(None) }.is_ok()
        }
        _ => false,
    };
    Some(VirtualFiles(Arc::new(Source { cookie, entries, started, wrote: AtomicBool::new(false) })))
}

impl VirtualSource for Source {
    fn entries(&self, _stop: &dyn Fn() -> bool) -> io::Result<Vec<VirtualEntry>> {
        Ok(self.entries.iter().map(|(_, entry)| entry.clone()).collect())
    }

    fn write(&self, index: usize, to: &Path, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<u64> {
        let (lindex, entry) = self.entries.get(index).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        if entry.is_dir || entry.failed.is_some() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Not a file the program can give"));
        }
        self.wrote.store(true, Ordering::Relaxed);
        let data = from_table(&table()?, self.cookie)?;
        let tymed = (TYMED_ISTREAM.0 | TYMED_HGLOBAL.0 | TYMED_ISTORAGE.0) as u32;
        let format = formatetc(format_id(CFSTR_FILECONTENTS), *lindex, tymed);
        let mut medium = unsafe { data.GetData(&format) }.map_err(io_error)?;
        let written = unsafe { write_medium(&medium, entry.size, to, progress) };
        unsafe { ReleaseStgMedium(&mut medium) };
        written
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        let Ok(table) = table() else { return };
        if self.started
            && let Ok(data) = from_table(&table, self.cookie)
            && let Ok(capability) = data.cast::<IDataObjectAsyncCapability>()
        {
            let (result, effect) =
                if self.wrote.load(Ordering::Relaxed) { (S_OK, DROPEFFECT_COPY) } else { (E_ABORT, DROPEFFECT_NONE) };
            let _ = unsafe { capability.EndOperation(result, None, effect.0) };
        }
        let _ = unsafe { table.RevokeInterfaceFromGlobal(self.cookie) };
    }
}

/// Writes `bytes` to `file` in pieces, telling `progress` (false: cancelled).
fn write_counted(
    file: &mut File,
    bytes: &[u8],
    done: &mut u64,
    progress: &mut dyn FnMut(u64) -> bool,
) -> io::Result<()> {
    for piece in bytes.chunks(CHUNK) {
        file.write_all(piece)?;
        *done += piece.len() as u64;
        if !progress(*done) {
            return Err(crate::fs::cancelled());
        }
    }
    Ok(())
}

/// Writes what `medium` holds to the new file `to`. Global memory is cut to `declared` (Explorer
/// rounds it up), never read past its own size; a stream is read to its end, however long.
unsafe fn write_medium(
    medium: &STGMEDIUM,
    declared: Option<u64>,
    to: &Path,
    progress: &mut dyn FnMut(u64) -> bool,
) -> io::Result<u64> {
    unsafe {
        if medium.tymed == TYMED_HGLOBAL.0 as u32 {
            let memory = medium.u.hGlobal;
            let size = GlobalSize(memory);
            let size = declared.map_or(size, |most| size.min(usize::try_from(most).unwrap_or(usize::MAX)));
            let mut file = File::create_new(to)?;
            let at = GlobalLock(memory);
            if at.is_null() {
                return Err(io::Error::other("The program's data could not be read"));
            }
            let mut done = 0;
            let result =
                write_counted(&mut file, std::slice::from_raw_parts(at.cast::<u8>(), size), &mut done, progress);
            let _ = GlobalUnlock(memory);
            result.map(|()| done)
        } else if medium.tymed == TYMED_ISTREAM.0 as u32 {
            let stream = (*medium.u.pstm).as_ref().ok_or_else(|| io::Error::other("No stream"))?;
            copy_stream(stream, to, progress)
        } else if medium.tymed == TYMED_ISTORAGE.0 as u32 {
            let storage = (*medium.u.pstg).as_ref().ok_or_else(|| io::Error::other("No storage"))?;
            copy_storage(storage, to)?;
            let size = std::fs::metadata(to)?.len();
            progress(size);
            Ok(size)
        } else {
            Err(io::Error::new(io::ErrorKind::Unsupported, "The program offers this item in a form Gezik cannot read"))
        }
    }
}

/// shortcut: a `Read` the source never answers cannot be cancelled (cancel is seen between
/// pieces); upgrade with `CoCancelCall` if a real source hangs there.
unsafe fn copy_stream(stream: &IStream, to: &Path, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<u64> {
    unsafe {
        // Some sources hand the stream at its end.
        let _ = stream.Seek(0, STREAM_SEEK_SET, None);
        let mut file = File::create_new(to)?;
        let mut buffer = vec![0u8; CHUNK];
        let mut done = 0;
        loop {
            let mut read = 0u32;
            stream.Read(buffer.as_mut_ptr().cast(), CHUNK as u32, Some(&mut read)).ok().map_err(io_error)?;
            // A source claiming more than it was asked for is refused, not trusted.
            let piece =
                buffer.get(..read as usize).ok_or_else(|| io::Error::other("The program's stream is broken"))?;
            if piece.is_empty() {
                return Ok(done);
            }
            write_counted(&mut file, piece, &mut done, progress)?;
        }
    }
}

/// An Outlook message (`.msg`): the storage copied whole into a new compound file at `to`
/// (no `STGM_CREATE`: an existing file is never written over).
unsafe fn copy_storage(storage: &IStorage, to: &Path) -> io::Result<()> {
    unsafe {
        let path = HSTRING::from(to.as_os_str());
        let mut out: Option<IStorage> = None;
        StgCreateStorageEx(
            &path,
            STGM_READWRITE | STGM_SHARE_EXCLUSIVE,
            STGFMT_STORAGE,
            0,
            None,
            None,
            &IStorage::IID,
            std::ptr::from_mut(&mut out).cast(),
        )
        .map_err(io_error)?;
        let out = out.ok_or_else(|| io::Error::other("No storage was made"))?;
        storage.CopyTo(None, None, &out).map_err(io_error)?;
        out.Commit(STGC_DEFAULT.0 as u32).map_err(io_error)
    }
}

#[cfg(test)]
mod tests {
    use std::mem::ManuallyDrop;

    use windows::Win32::Foundation::{DV_E_FORMATETC, E_FAIL, E_NOTIMPL};
    use windows::Win32::System::Com::StructuredStorage::StgCreateDocfile;
    use windows::Win32::System::Com::{
        COINIT_MULTITHREADED, IAdviseSink, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, STGM_CREATE,
        STGM_DELETEONRELEASE, STGMEDIUM_0,
    };
    use windows::Win32::UI::Shell::SHCreateMemStream;
    use windows_core::{BOOL, HRESULT, Ref, implement, w};

    use super::*;

    /// A `FILEGROUPDESCRIPTORW` of (name, folder, declared size).
    fn descriptor(items: &[(&str, bool, Option<u64>)]) -> Vec<u8> {
        let mut bytes = (items.len() as u32).to_le_bytes().to_vec();
        for (name, is_dir, size) in items {
            let mut d = vec![0u8; DESCRIPTOR];
            let flags = FD_ATTRIBUTES | if size.is_some() { FD_FILESIZE } else { 0 };
            d[0..4].copy_from_slice(&flags.to_le_bytes());
            d[36..40].copy_from_slice(&(if *is_dir { DIRECTORY } else { 0x80 }).to_le_bytes());
            let size = size.unwrap_or(0);
            d[64..68].copy_from_slice(&((size >> 32) as u32).to_le_bytes());
            d[68..72].copy_from_slice(&(size as u32).to_le_bytes());
            for (i, unit) in name.encode_utf16().take(260).enumerate() {
                d[NAME_AT + 2 * i..NAME_AT + 2 * i + 2].copy_from_slice(&unit.to_le_bytes());
            }
            bytes.extend(d);
        }
        bytes
    }

    fn entry(name: &str, is_dir: bool, size: Option<u64>) -> VirtualEntry {
        VirtualEntry { name: name.into(), is_dir, size, failed: None }
    }

    #[test]
    fn descriptors_are_read_as_far_as_the_bytes_go() {
        let bytes =
            descriptor(&[("rapor.pdf", false, Some(5)), ("Ekler", true, None), (r"Ekler\a.txt", false, Some(1 << 33))]);
        let expected = [
            entry("rapor.pdf", false, Some(5)),
            entry("Ekler", true, None),
            entry(r"Ekler\a.txt", false, Some(1 << 33)),
        ];
        assert_eq!(descriptor_entries(&bytes), expected);
        let mut lying = bytes.clone();
        lying[0..4].copy_from_slice(&1000u32.to_le_bytes());
        assert_eq!(descriptor_entries(&lying), expected, "a count past the bytes: only whole descriptors");
        lying[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(descriptor_entries(&lying), expected);
        assert_eq!(descriptor_entries(&bytes[..bytes.len() - 1]).len(), 2);
        assert!(descriptor_entries(&[1, 0]).is_empty());
        assert!(descriptor_entries(&[]).is_empty());
        let mut no_attributes = descriptor(&[("x", true, None)]);
        no_attributes[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert!(!descriptor_entries(&no_attributes)[0].is_dir, "the attributes count only when flagged");
        let longest = "a".repeat(259);
        assert_eq!(descriptor_entries(&descriptor(&[(&longest, false, None)])), [entry(&longest, false, None)]);
        let endless = descriptor_entries(&descriptor(&[(&"a".repeat(260), false, None)]));
        assert!(endless[0].failed.is_some(), "a name with no NUL is broken");
    }

    #[test]
    fn link_shortcuts_are_left_out_only_with_a_url() {
        let entries = vec![entry("Gezik.url", false, None), entry("resim.png", false, None)];
        assert_eq!(without_links(entries.clone(), true), [(1, entry("resim.png", false, None))]);
        assert_eq!(without_links(entries, false).len(), 2, "an attachment named .url stays");
        let tricky = vec![entry("x.url.", false, None), entry("y.URL ", false, None), entry(r"d\z.url", false, None)];
        assert!(without_links(tricky, true).is_empty(), "judged as written");
    }

    /// A data object as Outlook or a browser hands one: a descriptor, and the contents of item
    /// 0 as global memory, of item 1 as a stream, of item 2 as a storage.
    #[implement(IDataObject)]
    struct Fake {
        descriptor: Vec<u8>,
        bodies: Vec<Vec<u8>>,
    }

    fn global(bytes: &[u8]) -> windows_core::Result<STGMEDIUM> {
        let memory = crate::clipboard::global_copy(bytes).ok_or(windows_core::Error::from(E_FAIL))?;
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: memory },
            pUnkForRelease: ManuallyDrop::new(None),
        })
    }

    impl IDataObject_Impl for Fake_Impl {
        fn GetData(&self, format: *const FORMATETC) -> windows_core::Result<STGMEDIUM> {
            let format = unsafe { &*format };
            if format.cfFormat == format_id(CFSTR_FILEDESCRIPTORW) {
                return global(&self.descriptor);
            }
            let body = usize::try_from(format.lindex).ok().and_then(|i| self.bodies.get(i));
            let (true, Some(body)) = (format.cfFormat == format_id(CFSTR_FILECONTENTS), body) else {
                return Err(DV_E_FORMATETC.into());
            };
            unsafe {
                match format.lindex {
                    0 => global(body),
                    1 => {
                        let stream = SHCreateMemStream(Some(body)).ok_or(windows_core::Error::from(E_FAIL))?;
                        Ok(STGMEDIUM {
                            tymed: TYMED_ISTREAM.0 as u32,
                            u: STGMEDIUM_0 { pstm: ManuallyDrop::new(Some(stream)) },
                            pUnkForRelease: ManuallyDrop::new(None),
                        })
                    }
                    _ => {
                        let mode = STGM_CREATE | STGM_READWRITE | STGM_SHARE_EXCLUSIVE | STGM_DELETEONRELEASE;
                        let storage = StgCreateDocfile(None, mode, None)?;
                        let stream = storage.CreateStream(
                            w!("Body"),
                            STGM_CREATE | STGM_READWRITE | STGM_SHARE_EXCLUSIVE,
                            0,
                            0,
                        )?;
                        stream.Write(body.as_ptr().cast(), body.len() as u32, None).ok()?;
                        storage.Commit(0)?;
                        Ok(STGMEDIUM {
                            tymed: TYMED_ISTORAGE.0 as u32,
                            u: STGMEDIUM_0 { pstg: ManuallyDrop::new(Some(storage)) },
                            pUnkForRelease: ManuallyDrop::new(None),
                        })
                    }
                }
            }
        }
        fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> windows_core::Result<()> {
            Err(E_NOTIMPL.into())
        }
        fn QueryGetData(&self, _: *const FORMATETC) -> HRESULT {
            DV_E_FORMATETC
        }
        fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, _: *mut FORMATETC) -> HRESULT {
            E_NOTIMPL
        }
        fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> windows_core::Result<()> {
            Err(E_NOTIMPL.into())
        }
        fn EnumFormatEtc(&self, _: u32) -> windows_core::Result<IEnumFORMATETC> {
            Err(E_NOTIMPL.into())
        }
        fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Ref<IAdviseSink>) -> windows_core::Result<u32> {
            Err(E_NOTIMPL.into())
        }
        fn DUnadvise(&self, _: u32) -> windows_core::Result<()> {
            Err(E_NOTIMPL.into())
        }
        fn EnumDAdvise(&self) -> windows_core::Result<IEnumSTATDATA> {
            Err(E_NOTIMPL.into())
        }
    }

    fn fake() -> IDataObject {
        Fake {
            descriptor: descriptor(&[
                ("a.txt", false, Some(3)),
                ("b.bin", false, None),
                ("c.msg", false, None),
                ("Ekler", true, None),
            ]),
            bodies: vec![b"hello".to_vec(), vec![7u8; 600_000], b"msg".to_vec()],
        }
        .into()
    }

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-virtual-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn hglobal_stream_and_storage_items_are_written() {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let dir = test_dir("kinds");
        let data = fake();
        assert_eq!(offered(&data).len(), 4);
        let files = files(&data).expect("offered");
        assert_eq!(files.0.entries(&|| false).unwrap().len(), 4);
        // From another thread, as the job does.
        let worker = {
            let (files, dir) = (files.clone(), dir.clone());
            std::thread::spawn(move || {
                (0..3)
                    .map(|i| {
                        let mut last = 0;
                        let n = files.0.write(i, &dir.join(i.to_string()), &mut |done| {
                            last = done;
                            true
                        });
                        (n.unwrap(), last)
                    })
                    .collect::<Vec<_>>()
            })
        };
        let written = worker.join().unwrap();
        assert_eq!(std::fs::read(dir.join("0")).unwrap(), b"hel", "cut to the declared size");
        assert_eq!(written[0], (3, 3));
        assert_eq!(std::fs::read(dir.join("1")).unwrap(), vec![7u8; 600_000]);
        assert_eq!(written[1], (600_000, 600_000));
        assert!(std::fs::read(dir.join("2")).unwrap().starts_with(&[0xD0, 0xCF, 0x11, 0xE0]), "a compound file");
        assert!(files.0.write(3, &dir.join("3"), &mut |_| true).is_err(), "a folder is no file to write");
        assert!(!dir.join("3").exists());
        drop(files);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_existing_file_is_never_written_over() {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let dir = test_dir("existing");
        std::fs::write(dir.join("taken"), "mine").unwrap();
        let files = files(&fake()).unwrap();
        for i in 0..3 {
            assert!(files.0.write(i, &dir.join("taken"), &mut |_| true).is_err(), "item {i}");
        }
        assert_eq!(std::fs::read_to_string(dir.join("taken")).unwrap(), "mine");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn progress_can_cancel_a_write() {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let dir = test_dir("cancel");
        let files = files(&fake()).unwrap();
        let mut calls = 0;
        let err = files
            .0
            .write(1, &dir.join("x"), &mut |_| {
                calls += 1;
                false
            })
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert_eq!(calls, 1, "stopped at the first piece");
        assert!(files.0.write(9, &dir.join("y"), &mut |_| true).is_err(), "an index past the items");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

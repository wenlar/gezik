//! Windows: Gezik's own OLE drop target in place of winit's (which only reports paths, with
//! no position or keys), and the Shell's drag image over the window.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{
    DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, GlobalFree, HWND, LPARAM, POINT, POINTL, RECT,
    WPARAM,
};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, STGMEDIUM_0,
    TYMED_HGLOBAL,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{
    CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE, IDropSource,
    IDropSource_Impl, IDropTarget, IDropTarget_Impl, MK_ALT, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop,
};
use windows::Win32::System::SystemServices::{MK_CONTROL, MK_LBUTTON, MK_RBUTTON, MK_SHIFT, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Input::KeyboardAndMouse::SetCapture;
use windows::Win32::UI::Shell::{
    CFSTR_DROPDESCRIPTION, CFSTR_LOGICALPERFORMEDDROPEFFECT, CFSTR_PERFORMEDDROPEFFECT, CLSID_DragDropHelper,
    DROPDESCRIPTION, DROPIMAGE_COPY, DROPIMAGE_INVALID, DROPIMAGE_LINK, DROPIMAGE_MOVE, DROPIMAGE_NONE, HDROP,
    IDropTargetHelper, SHDoDragDrop,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetClientRect, GetCursorPos, PostMessageW, WM_LBUTTONUP, WM_RBUTTONUP, WindowFromPoint,
};
use windows_core::{BOOL, HRESULT, PCWSTR, Ref, implement};

use super::{Allowed, Answer, DragEnd, DropHandler, Effect, Keys, Offer};

/// What `Drop` answers for `effect`: the effect returned to the source, then the "Performed
/// DropEffect" and "Logical Performed DropEffect" written to the data object. A move is an
/// optimized move: Gezik moves the files, so the source is told nothing was moved and must
/// not delete them (Microsoft, "Handling Shell Data Transfer Scenarios").
pub(crate) fn drop_reply(effect: Option<Effect>) -> (DROPEFFECT, Option<DROPEFFECT>, Option<DROPEFFECT>) {
    match effect {
        Some(Effect::Move) => (DROPEFFECT_NONE, Some(DROPEFFECT_NONE), Some(DROPEFFECT_MOVE)),
        Some(Effect::Copy) => (DROPEFFECT_COPY, None, None),
        Some(Effect::Link) => (DROPEFFECT_LINK, None, None),
        None => (DROPEFFECT_NONE, None, None),
    }
}

fn to_dropeffect(effect: Option<Effect>) -> DROPEFFECT {
    match effect {
        Some(Effect::Move) => DROPEFFECT_MOVE,
        Some(Effect::Copy) => DROPEFFECT_COPY,
        Some(Effect::Link) => DROPEFFECT_LINK,
        None => DROPEFFECT_NONE,
    }
}

/// The text the Shell shows on the drag image for `answer`: "Move to %1" with the folder
/// inserted; `None` hands the text back to the source (the pointer left the window).
pub(crate) fn description(answer: Option<&Answer>) -> DROPDESCRIPTION {
    let mut out = DROPDESCRIPTION { r#type: DROPIMAGE_INVALID, ..Default::default() };
    let Some(answer) = answer else { return out };
    let (kind, message) = match answer.effect {
        Some(Effect::Move) => (DROPIMAGE_MOVE, "Move to %1"),
        Some(Effect::Copy) => (DROPIMAGE_COPY, "Copy to %1"),
        Some(Effect::Link) => (DROPIMAGE_LINK, "Create link in %1"),
        None => (DROPIMAGE_NONE, ""),
    };
    out.r#type = kind;
    if answer.effect.is_some()
        && let Some(folder) = &answer.folder
    {
        out.szMessage = wide(message);
        out.szInsert = wide(folder);
    }
    out
}

/// `text` as a fixed wide buffer (the struct is packed: its fields are assigned whole), cut
/// to fit with its terminating NUL.
fn wide(text: &str) -> [u16; 260] {
    let mut buffer = [0u16; 260];
    let units: Vec<u16> = text.encode_utf16().take(buffer.len() - 1).collect();
    buffer[..units.len()].copy_from_slice(&units);
    buffer
}

fn allowed_by(effects: DROPEFFECT) -> Allowed {
    let has = |effect: DROPEFFECT| effects.0 & effect.0 != 0;
    Allowed { copy: has(DROPEFFECT_COPY), move_: has(DROPEFFECT_MOVE), link: has(DROPEFFECT_LINK) }
}

fn keys_of(state: MODIFIERKEYS_FLAGS) -> Keys {
    let held = |bit: u32| state.0 & bit != 0;
    gezik_core::drag::keys_of(
        gezik_core::drag::DragOs::Windows,
        held(MK_SHIFT.0),
        held(MK_CONTROL.0),
        held(MK_ALT),
        false,
    )
}

fn format(id: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: id,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

/// The file paths in `data` (its `CF_HDROP`), if any.
fn paths_in(data: &IDataObject) -> Vec<std::path::PathBuf> {
    unsafe {
        let Ok(mut medium) = data.GetData(&format(CF_HDROP.0)) else { return Vec::new() };
        let paths = crate::clipboard::hdrop_paths(HDROP(medium.u.hGlobal.0));
        ReleaseStgMedium(&mut medium);
        paths
    }
}

/// Writes a drop effect to `data`.
fn set_effect(data: &IDataObject, name: PCWSTR, effect: DROPEFFECT) {
    set_bytes(data, name, &effect.0.to_le_bytes());
}

/// Writes the drag image's text to `data`.
fn set_description(data: &IDataObject, description: &DROPDESCRIPTION) {
    // A plain C struct: its bytes are what the Shell reads.
    let bytes = unsafe {
        std::slice::from_raw_parts((description as *const DROPDESCRIPTION).cast::<u8>(), size_of::<DROPDESCRIPTION>())
    };
    set_bytes(data, CFSTR_DROPDESCRIPTION, bytes);
}

/// Writes format `name` to `data` as global memory, which the data object owns after.
fn set_bytes(data: &IDataObject, name: PCWSTR, bytes: &[u8]) {
    unsafe {
        let id = RegisterClipboardFormatW(name) as u16;
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, bytes.len()) else { return };
        let target = GlobalLock(memory) as *mut u8;
        if target.is_null() {
            let _ = GlobalFree(Some(memory));
            return;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
        let _ = GlobalUnlock(memory);
        let medium = STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: memory },
            pUnkForRelease: std::mem::ManuallyDrop::new(None),
        };
        if data.SetData(&format(id), &medium, true).is_err() {
            let _ = GlobalFree(Some(memory));
        }
    }
}

#[implement(IDropTarget)]
struct Target {
    hwnd: HWND,
    handler: Rc<dyn DropHandler>,
    /// What is over the window now (read once, on entry), and its data object (for the
    /// drag image's text).
    offer: RefCell<Option<Offer>>,
    data: RefCell<Option<IDataObject>>,
    /// The last answer, so the drag image's text is only written when it changes.
    answer: RefCell<Option<Answer>>,
    /// Draws the source's drag image over the window.
    helper: Option<IDropTargetHelper>,
}

impl Target {
    /// Tells the source's drag image what a drop would do, when that changed.
    fn describe(&self, answer: Option<Answer>) {
        if *self.answer.borrow() == answer {
            return;
        }
        if let Some(data) = &*self.data.borrow() {
            set_description(data, &description(answer.as_ref()));
        }
        *self.answer.borrow_mut() = answer;
    }

    fn client_point(&self, pt: &POINTL) -> (POINT, f64, f64) {
        let screen = POINT { x: pt.x, y: pt.y };
        let mut client = screen;
        let _ = unsafe { ScreenToClient(self.hwnd, &mut client) };
        (screen, f64::from(client.x), f64::from(client.y))
    }
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        state: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows_core::Result<()> {
        let (screen, x, y) = self.client_point(pt);
        let offered = unsafe { *effect };
        let answer = match data.as_ref() {
            Some(data) => {
                let paths = paths_in(data);
                *self.data.borrow_mut() = Some(data.clone());
                if paths.is_empty() {
                    *self.offer.borrow_mut() = None;
                    Answer::default()
                } else {
                    let offer = Offer { paths, allowed: allowed_by(offered), right: state.0 & MK_RBUTTON.0 != 0 };
                    let answer = self.handler.over(&offer, x, y, keys_of(state));
                    *self.offer.borrow_mut() = Some(offer);
                    answer
                }
            }
            None => Answer::default(),
        };
        let reply = to_dropeffect(answer.effect);
        self.describe(Some(answer));
        unsafe { *effect = reply };
        if let (Some(helper), Some(data)) = (&self.helper, data.as_ref()) {
            let _ = unsafe { helper.DragEnter(self.hwnd, data, &screen, reply) };
        }
        Ok(())
    }

    fn DragOver(&self, state: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows_core::Result<()> {
        let (screen, x, y) = self.client_point(pt);
        let offered = unsafe { *effect };
        let answer = match &mut *self.offer.borrow_mut() {
            Some(offer) => {
                offer.allowed = allowed_by(offered);
                self.handler.over(offer, x, y, keys_of(state))
            }
            None => Answer::default(),
        };
        let reply = to_dropeffect(answer.effect);
        self.describe(Some(answer));
        unsafe { *effect = reply };
        if let Some(helper) = &self.helper {
            let _ = unsafe { helper.DragOver(&screen, reply) };
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows_core::Result<()> {
        if self.offer.borrow_mut().take().is_some() {
            self.handler.leave();
        }
        self.describe(None);
        self.data.borrow_mut().take();
        if let Some(helper) = &self.helper {
            let _ = unsafe { helper.DragLeave() };
        }
        Ok(())
    }

    fn Drop(
        &self,
        data: Ref<IDataObject>,
        state: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows_core::Result<()> {
        let (screen, x, y) = self.client_point(pt);
        let offered = unsafe { *effect };
        let offer = self.offer.borrow_mut().take();
        let done = offer.and_then(|mut offer| {
            offer.allowed = allowed_by(offered);
            self.handler.dropped(&offer, x, y, keys_of(state))
        });
        let (reply, performed, logical) = drop_reply(done);
        self.answer.borrow_mut().take();
        self.data.borrow_mut().take();
        if let Some(data) = data.as_ref() {
            if let Some(performed) = performed {
                set_effect(data, CFSTR_PERFORMEDDROPEFFECT, performed);
            }
            if let Some(logical) = logical {
                set_effect(data, CFSTR_LOGICALPERFORMEDDROPEFFECT, logical);
            }
        }
        unsafe { *effect = reply };
        if let (Some(helper), Some(data)) = (&self.helper, data.as_ref()) {
            let _ = unsafe { helper.Drop(data, &screen, reply) };
        }
        Ok(())
    }
}

/// Gezik's drop target on the window, until dropped.
pub struct Registration {
    hwnd: HWND,
}

impl Registration {
    /// Runs the system's drag loop for `paths` (all in one folder), with the Shell's own data
    /// object and drag image, as Explorer does; blocks until it ends. Gezik never deletes
    /// anything here: a target that moves does so itself.
    pub fn drag_out(&self, paths: &[PathBuf], right: bool) -> Result<DragEnd, String> {
        let (folder, names) = crate::shell_menu::shared_parent(paths).ok_or("the items are not in one folder")?;
        let data: IDataObject =
            unsafe { crate::shell_menu::children_object(self.hwnd, folder, &names) }.map_err(|e| e.to_string())?;
        let source = Rc::new(SourceState { hwnd: self.hwnd, right, ended: Cell::new(Next::Go) });
        let drop_source: IDropSource = Source(source.clone()).into();
        let effects = DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0);
        // Its result cannot tell a cancel from an optimized move (both "none"), so the source
        // remembers how the loop ended.
        let result = unsafe { SHDoDragDrop(Some(self.hwnd), &data, &drop_source, effects) };
        if source.ended.get() == Next::Returned {
            // The button is still down: the window takes the mouse back, so the drag goes on
            // in Gezik even when the pointer leaves again.
            unsafe { SetCapture(self.hwnd) };
            return Ok(DragEnd::Returned);
        }
        // winit never saw the button come up (the drag loop took it): tell it, so it and
        // Slint end the press.
        let mut cursor = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut cursor);
            let _ = ScreenToClient(self.hwnd, &mut cursor);
            let lparam = ((cursor.y as u32 & 0xFFFF) << 16) | (cursor.x as u32 & 0xFFFF);
            let message = if right { WM_RBUTTONUP } else { WM_LBUTTONUP };
            let _ = PostMessageW(Some(self.hwnd), message, WPARAM(0), LPARAM(lparam as isize));
        }
        match (result, source.ended.get()) {
            (Err(err), _) => Err(err.to_string()),
            (Ok(_), Next::Drop) => Ok(DragEnd::Dropped),
            (Ok(_), _) => Ok(DragEnd::Cancelled),
        }
    }
}

/// Whether the cursor is over Gezik's window `hwnd`, inside its client area.
fn over_window(hwnd: HWND) -> bool {
    unsafe {
        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_err() || GetAncestor(WindowFromPoint(cursor), GA_ROOT) != hwnd {
            return false;
        }
        let mut client = cursor;
        let _ = ScreenToClient(hwnd, &mut client);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        client.x >= rect.left && client.x < rect.right && client.y >= rect.top && client.y < rect.bottom
    }
}

struct SourceState {
    hwnd: HWND,
    right: bool,
    /// How the drag loop ended (`Go` while it runs).
    ended: Cell<Next>,
}

#[implement(IDropSource)]
struct Source(Rc<SourceState>);

impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, state: MODIFIERKEYS_FLAGS) -> HRESULT {
        let button = if self.0.right { MK_RBUTTON } else { MK_LBUTTON };
        let down = state.0 & button.0 != 0;
        // Asked only while the button is down: a release over the window drops on it.
        let over = down && over_window(self.0.hwnd);
        let next = next_step(escape.as_bool(), down, over);
        self.0.ended.set(next);
        match next {
            Next::Go => HRESULT(0),
            Next::Drop => DRAGDROP_S_DROP,
            Next::Cancel | Next::Returned => DRAGDROP_S_CANCEL,
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let _ = unsafe { RevokeDragDrop(self.hwnd) };
    }
}

/// Puts Gezik's drop target on `window` in place of winit's. OLE is already initialized on
/// this thread (winit does it for its own drop target).
pub fn register(window: &impl HasWindowHandle, handler: Rc<dyn DropHandler>) -> Option<Registration> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else { return None };
    let hwnd = HWND(win32.hwnd.get() as *mut _);
    let helper: Option<IDropTargetHelper> =
        unsafe { CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER) }.ok();
    let target: IDropTarget = Target {
        hwnd,
        handler,
        offer: RefCell::new(None),
        data: RefCell::new(None),
        answer: RefCell::new(None),
        helper,
    }
    .into();
    unsafe {
        let _ = RevokeDragDrop(hwnd);
        if let Err(err) = RegisterDragDrop(hwnd, &target) {
            eprintln!("gezik: cannot take dropped files: {err}");
            return None;
        }
    }
    Some(Registration { hwnd })
}

/// What the system's drag loop does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Next {
    Go,
    Drop,
    Cancel,
    /// Back over Gezik's window with the button down: Gezik takes the drag back.
    Returned,
}

/// `IDropSource::QueryContinueDrag`: Esc cancels, releasing the button drops (also over
/// Gezik's own window: its drop target takes it), coming back over Gezik's window hands the
/// drag back to Gezik.
pub(crate) fn next_step(escape: bool, button_down: bool, over_gezik: bool) -> Next {
    if escape {
        Next::Cancel
    } else if !button_down {
        Next::Drop
    } else if over_gezik {
        Next::Returned
    } else {
        Next::Go
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drag_loop_ends_as_the_pointer_says() {
        assert_eq!(next_step(false, true, false), Next::Go);
        assert_eq!(next_step(true, true, false), Next::Cancel);
        assert_eq!(next_step(true, false, true), Next::Cancel, "Esc wins");
        assert_eq!(next_step(false, false, false), Next::Drop);
        assert_eq!(next_step(false, false, true), Next::Drop, "released at once over Gezik: its target drops");
        assert_eq!(next_step(false, true, true), Next::Returned);
    }

    #[test]
    fn move_drop_reports_an_optimized_move() {
        assert_eq!(drop_reply(Some(Effect::Move)), (DROPEFFECT_NONE, Some(DROPEFFECT_NONE), Some(DROPEFFECT_MOVE)));
        assert_eq!(drop_reply(Some(Effect::Copy)), (DROPEFFECT_COPY, None, None));
        assert_eq!(drop_reply(Some(Effect::Link)), (DROPEFFECT_LINK, None, None));
        assert_eq!(to_dropeffect(Some(Effect::Link)), DROPEFFECT_LINK);
        assert_eq!(drop_reply(None), (DROPEFFECT_NONE, None, None));
    }

    #[test]
    fn the_drag_image_names_the_folder() {
        let text = |units: &[u16; 260]| String::from_utf16_lossy(&units[..units.iter().position(|&u| u == 0).unwrap()]);
        // Fields of the packed struct are copied out before use.
        let parts = |d: DROPDESCRIPTION| {
            let (kind, message, insert) = (d.r#type, d.szMessage, d.szInsert);
            (kind, text(&message), text(&insert))
        };
        let d = description(Some(&Answer { effect: Some(Effect::Move), folder: Some("Belgeler".into()) }));
        assert_eq!(parts(d), (DROPIMAGE_MOVE, "Move to %1".into(), "Belgeler".into()));
        let d = description(Some(&Answer { effect: Some(Effect::Link), folder: Some("Belgeler".into()) }));
        let kind = d.r#type;
        assert_eq!(kind, DROPIMAGE_LINK);
        let refused = description(Some(&Answer { effect: None, folder: Some("x".into()) }));
        assert_eq!(parts(refused), (DROPIMAGE_NONE, String::new(), String::new()));
        assert_eq!(parts(description(None)).0, DROPIMAGE_INVALID);
        let long = description(Some(&Answer { effect: Some(Effect::Copy), folder: Some("ş".repeat(400)) }));
        assert_eq!(parts(long).2.chars().count(), 259, "cut to fit");
    }

    #[test]
    fn keys_and_allowed_effects_are_read() {
        assert_eq!(
            keys_of(MODIFIERKEYS_FLAGS(MK_SHIFT.0 | MK_CONTROL.0)),
            Keys { shift: true, copy: true, link: true }
        );
        assert_eq!(keys_of(MODIFIERKEYS_FLAGS(MK_ALT)), Keys { shift: false, copy: false, link: true });
        assert_eq!(allowed_by(DROPEFFECT_COPY), Allowed { copy: true, move_: false, link: false });
        assert_eq!(allowed_by(DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0 | DROPEFFECT_LINK.0)), Allowed::ALL);
    }
}

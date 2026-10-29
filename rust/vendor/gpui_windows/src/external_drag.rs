// OpenVector addition: copy-only OLE file dragging. No clipboard or file ownership transfer.
#![deny(unsafe_op_in_unsafe_fn)]

use std::{cell::Cell, os::windows::ffi::OsStrExt, rc::Rc};

use gpui::{ExternalDragOutcome, ExternalDragPayload, FileDropEvent, PlatformInput};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, Memory::*, Ole::*, SystemServices::*},
        UI::{Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{implement, BOOL, HRESULT, Owned, Ref},
};

use crate::window::WindowsWindowInner;

thread_local! {
    // OLE pumps foreground tasks too. Do not allow another window to nest a source session.
    static DRAG_RESERVED: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn can_start(window: &WindowsWindowInner) -> bool {
    !window.state.external_drag_closed.get() && !DRAG_RESERVED.get()
}

pub(crate) fn start(window: Rc<WindowsWindowInner>, hwnd: HWND, payload: &ExternalDragPayload) -> bool {
    if !can_start(&window) {
        return false;
    }
    let ExternalDragPayload::Files(paths) = payload;
    if paths.entries().is_empty() {
        return false;
    }
    let mut names = Vec::new();
    for (path, _) in paths.entries() {
        // Absolute existing paths are the public payload boundary; never manufacture virtual files.
        if !path.is_absolute() || !path.exists() {
            return false;
        }
        let start = names.len();
        names.extend(path.as_os_str().encode_wide());
        if names[start..].contains(&0) {
            return false;
        }
        names.push(0);
    }
    names.push(0);
    let reservation = gpui_util::defer(|| DRAG_RESERVED.set(false));
    DRAG_RESERVED.set(true);
    let executor = window.executor.clone();
    executor.spawn(async move {
        let started = Rc::new(Cell::new(false));
        let outcome = if window.state.external_drag_closed.get() || !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            ExternalDragOutcome::Failed("The drag source closed before OLE startup".into())
        } else if unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } >= 0 {
            ExternalDragOutcome::Failed("The drag gesture ended before OLE startup".into())
        } else {
            let data: IDataObject = FileDataObject { names }.into();
            let source: IDropSource = FileDropSource { window: window.clone(), started: started.clone() }.into();
            let mut effect = DROPEFFECT_NONE;
            // No App/Window RefCell borrow is held across this nested native message loop.
            let status = unsafe { DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect) };
            if status == DRAGDROP_S_DROP && effect == DROPEFFECT_COPY {
                ExternalDragOutcome::Dropped
            } else if status != DRAGDROP_S_CANCEL && status != DRAGDROP_S_DROP {
                ExternalDragOutcome::Failed(format!("OLE file drag failed: {status}"))
            } else if !started.get() {
                ExternalDragOutcome::Failed("OLE ended before taking ownership of the drag gesture".into())
            } else {
                ExternalDragOutcome::Cancelled
            }
            // The receiver owns each returned STGMEDIUM and may retain IDataObject.
            // Neither this object nor the session removes any application-owned files.
        };
        drop(reservation);
        emit(&window, FileDropEvent::Ended { outcome });
    }).detach();
    true
}


fn emit(window: &WindowsWindowInner, event: FileDropEvent) {
    // Take the callback out of its Cell before reentering GPUI, matching inbound events.
    if let Some(mut callback) = window.state.callbacks.input.take() {
        callback(PlatformInput::FileDrop(event));
        window.state.callbacks.input.set(Some(callback));
    }
}

#[implement(IDropSource)]
struct FileDropSource {
    window: Rc<WindowsWindowInner>,
    started: Rc<Cell<bool>>,
}

#[allow(non_snake_case)]
impl IDropSource_Impl for FileDropSource_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() || self.window.state.external_drag_closed.get() {
            DRAGDROP_S_CANCEL
        } else if !keys.contains(MK_LBUTTON) {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        // Unlike queuing a task (or the initial QueryContinueDrag), feedback means
        // OLE has entered its drag loop. GPUI may now minimize the source window.
        if !self.started.replace(true) {
            emit(&self.window, FileDropEvent::Started);
        }
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

fn file_format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

fn check_format(format: *const FORMATETC) -> HRESULT {
    let Some(format) = (unsafe { format.as_ref() }) else { return E_POINTER };
    if format.cfFormat != CF_HDROP.0 {
        DV_E_FORMATETC
    } else if format.dwAspect != DVASPECT_CONTENT.0 {
        DV_E_DVASPECT
    } else if format.lindex != -1 {
        DV_E_LINDEX
    } else if format.tymed & TYMED_HGLOBAL.0 as u32 == 0 {
        DV_E_TYMED
    } else if !format.ptd.is_null() {
        DV_E_DVTARGETDEVICE
    } else {
        S_OK
    }
}

#[implement(IDataObject)]
struct FileDataObject {
    // UTF-16 preserves Windows filenames, including unpaired surrogates.
    names: Vec<u16>,
}

#[allow(non_snake_case)]
impl IDataObject_Impl for FileDataObject_Impl {
    fn GetData(&self, format: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        check_format(format).ok()?;
        let header = DROPFILES {
            pFiles: size_of::<DROPFILES>() as u32,
            fWide: true.into(),
            ..Default::default()
        };
        let size = size_of::<DROPFILES>() + size_of_val(self.names.as_slice());
        unsafe {
            // Every GetData allocates a distinct medium. Owned frees it on every error path.
            let global = Owned::new(GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size)?);
            let bytes = GlobalLock(*global).cast::<u8>();
            if bytes.is_null() {
                return Err(windows::core::Error::from_thread());
            }
            bytes.cast::<DROPFILES>().write(header);
            std::ptr::copy_nonoverlapping(self.names.as_ptr(), bytes.add(size_of::<DROPFILES>()).cast::<u16>(), self.names.len());
            let _ = GlobalUnlock(*global);
            let medium = STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: *global },
                pUnkForRelease: Default::default(),
            };
            // The caller now releases this allocation with ReleaseStgMedium.
            std::mem::forget(global);
            Ok(medium)
        }
    }

    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> windows::core::Result<()> {
        Err(DV_E_TYMED.into())
    }

    fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
        check_format(format)
    }

    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, output: *mut FORMATETC) -> HRESULT {
        let Some(output) = (unsafe { output.as_mut() }) else { return E_POINTER };
        output.ptd = std::ptr::null_mut();
        E_NOTIMPL
    }

    fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, direction: u32) -> windows::core::Result<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        unsafe { SHCreateStdEnumFmtEtc(&[file_format()]) }
    }

    fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Ref<IAdviseSink>) -> windows::core::Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _: u32) -> windows::core::Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdrop_preserves_utf16_and_returns_independently_owned_media() {
        // Include spaces, non-BMP text, and a lone surrogate, all legal in Windows paths.
        let first: Vec<u16> = r"C:\export folder\猫𐐀.svg".encode_utf16().collect();
        let mut second: Vec<u16> = r"C:\export folder\".encode_utf16().collect();
        second.extend([0xd800, b'.' as u16, b's' as u16, b'v' as u16, b'g' as u16]);
        let names = first.iter().copied().chain([0])
            .chain(second.iter().copied()).chain([0, 0]).collect();
        let data: IDataObject = FileDataObject { names }.into();
        unsafe {
            let mut a = data.GetData(&file_format()).unwrap();
            let mut b = data.GetData(&file_format()).unwrap();
            assert_ne!(a.u.hGlobal, b.u.hGlobal);
            ReleaseStgMedium(&mut a);
            drop(data);

            // The receiver's second medium must survive both the first release and object release.
            let hdrop = HDROP(b.u.hGlobal.0);
            assert_eq!(DragQueryFileW(hdrop, u32::MAX, None), 2);
            for (index, expected) in [first, second].iter().enumerate() {
                let length = DragQueryFileW(hdrop, index as u32, None) as usize;
                assert_eq!(length, expected.len());
                let mut actual = vec![0; length + 1];
                assert_eq!(DragQueryFileW(hdrop, index as u32, Some(&mut actual)), length as u32);
                assert_eq!(&actual[..length], expected.as_slice());
                assert_eq!(actual[length], 0);
            }
            ReleaseStgMedium(&mut b);
        }
    }
}

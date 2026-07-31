//! Audited RAII ownership for native values used by Shell adapters.
#![allow(
    unsafe_code,
    reason = "native RAII must call the allocator-specific Windows cleanup APIs"
)]

use std::{
    ffi::c_void,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

static OWNED_BITMAPS: AtomicUsize = AtomicUsize::new(0);
static OWNED_DCS: AtomicUsize = AtomicUsize::new(0);
static OWNED_ICONS: AtomicUsize = AtomicUsize::new(0);
static OWNED_HANDLES: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeResourceSnapshot {
    pub bitmaps: usize,
    pub device_contexts: usize,
    pub icons: usize,
    pub kernel_handles: usize,
}

impl NativeResourceSnapshot {
    pub fn capture() -> Self {
        Self {
            bitmaps: OWNED_BITMAPS.load(Ordering::Acquire),
            device_contexts: OWNED_DCS.load(Ordering::Acquire),
            icons: OWNED_ICONS.load(Ordering::Acquire),
            kernel_handles: OWNED_HANDLES.load(Ordering::Acquire),
        }
    }
}

use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Graphics::Gdi::{DeleteDC, DeleteObject, HBITMAP, HDC, HGDIOBJ},
    System::Com::CoTaskMemFree,
    UI::WindowsAndMessaging::{DestroyIcon, HICON},
};

/// Unique GDI bitmap returned by `IShellItemImageFactory::GetImage`.
pub(crate) struct OwnedBitmap(HBITMAP);

impl OwnedBitmap {
    /// # Safety
    /// `bitmap` must be a uniquely owned non-null GDI object requiring `DeleteObject`.
    pub(crate) unsafe fn from_raw(bitmap: HBITMAP) -> Option<Self> {
        if bitmap.0.is_null() {
            None
        } else {
            OWNED_BITMAPS.fetch_add(1, Ordering::AcqRel);
            Some(Self(bitmap))
        }
    }

    pub(crate) const fn get(&self) -> HBITMAP {
        self.0
    }
}

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        // SAFETY: ownership is unique and GetImage documents DeleteObject cleanup.
        let _ = unsafe { DeleteObject(HGDIOBJ::from(self.0)) };
        OWNED_BITMAPS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Unique compatible device context used only for synchronous bitmap readback.
pub(crate) struct OwnedDc(HDC);

impl OwnedDc {
    /// # Safety
    /// `dc` must be a uniquely owned non-null DC requiring `DeleteDC`.
    pub(crate) unsafe fn from_raw(dc: HDC) -> Option<Self> {
        if dc.0.is_null() {
            None
        } else {
            OWNED_DCS.fetch_add(1, Ordering::AcqRel);
            Some(Self(dc))
        }
    }

    pub(crate) const fn get(&self) -> HDC {
        self.0
    }
}

impl Drop for OwnedDc {
    fn drop(&mut self) {
        // SAFETY: ownership is unique and CreateCompatibleDC documents DeleteDC cleanup.
        let _ = unsafe { DeleteDC(self.0) };
        OWNED_DCS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Unique icon returned by `SHGetFileInfoW` when `SHGFI_ICON` is requested.
pub(crate) struct OwnedIcon(HICON);

impl OwnedIcon {
    /// # Safety
    /// `icon` must be a uniquely owned non-null icon whose API contract requires `DestroyIcon`.
    pub(crate) unsafe fn from_raw(icon: HICON) -> Option<Self> {
        if icon.0.is_null() {
            None
        } else {
            OWNED_ICONS.fetch_add(1, Ordering::AcqRel);
            Some(Self(icon))
        }
    }

    pub(crate) const fn get(&self) -> HICON {
        self.0
    }
}

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        // SAFETY: SHGetFileInfoW transferred unique ownership and this wrapper drops exactly once.
        let _ = unsafe { DestroyIcon(self.0) };
        OWNED_ICONS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Unique ownership of memory returned by an API documented to use `CoTaskMemAlloc`.
///
/// The value may be created only at the exact ownership-transfer point. It must remain in the
/// originating apartment when `T` itself is apartment-affine; dropping always calls
/// `CoTaskMemFree` exactly once.
pub(crate) struct CoTaskMem<T>(NonNull<T>);

impl<T> CoTaskMem<T> {
    /// Takes ownership of a non-null COM task allocator pointer.
    ///
    /// # Safety
    ///
    /// `raw` must be uniquely owned, allocated with the COM task allocator, and valid for `T`.
    pub(crate) unsafe fn from_raw(raw: *mut T) -> Option<Self> {
        NonNull::new(raw).map(Self)
    }

    pub(crate) fn as_ptr(&self) -> *const T {
        self.0.as_ptr()
    }
}

impl<T> Drop for CoTaskMem<T> {
    fn drop(&mut self) {
        // SAFETY: the constructor contract guarantees unique CoTaskMem ownership.
        unsafe { CoTaskMemFree(Some(self.0.as_ptr().cast::<c_void>())) };
    }
}

/// Unique ownership of a closeable kernel handle.
pub(crate) struct OwnedHandle(HANDLE);

impl OwnedHandle {
    /// Takes ownership of a valid handle whose API contract requires `CloseHandle`.
    ///
    /// # Safety
    ///
    /// The caller must own `handle`, must not close it elsewhere, and the originating API must
    /// document `CloseHandle` as its cleanup function.
    pub(crate) unsafe fn from_raw(handle: HANDLE) -> Option<Self> {
        if handle.is_invalid() {
            None
        } else {
            OWNED_HANDLES.fetch_add(1, Ordering::AcqRel);
            Some(Self(handle))
        }
    }

    pub(crate) const fn get(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: OwnedHandle's constructor contract guarantees unique CloseHandle ownership.
        let _ = unsafe { CloseHandle(self.0) };
        OWNED_HANDLES.fetch_sub(1, Ordering::AcqRel);
    }
}

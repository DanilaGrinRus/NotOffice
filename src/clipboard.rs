//! Explicit, bounded Unicode-text clipboard access for the paste command.
//!
//! The clipboard is never polled. A read happens only after the user presses
//! the explicit range-paste action.
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardError {
    Unavailable,
    Empty,
    TooLarge,
    UnsupportedPlatform,
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => write!(f, "clipboard is unavailable or busy"),
            Self::Empty => write!(f, "clipboard does not contain text"),
            Self::TooLarge => write!(f, "clipboard text exceeds the paste limit"),
            Self::UnsupportedPlatform => {
                write!(f, "clipboard range paste is implemented for Windows only")
            }
        }
    }
}

pub const MAX_CLIPBOARD_TEXT_BYTES: usize = 4 * 1024 * 1024;

#[cfg(windows)]
pub fn read_text() -> Result<String, ClipboardError> {
    use std::ptr;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return Err(ClipboardError::Unavailable);
        }
        let result = (|| {
            if IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT)) == 0 {
                return Err(ClipboardError::Empty);
            }
            let handle: HANDLE = GetClipboardData(u32::from(CF_UNICODETEXT));
            if handle.is_null() {
                return Err(ClipboardError::Unavailable);
            }
            let pointer = GlobalLock(handle) as *const u16;
            if pointer.is_null() {
                return Err(ClipboardError::Unavailable);
            }
            // Never scan past the HGLOBAL allocation. A hostile clipboard
            // owner can publish CF_UNICODETEXT without a terminator; trusting
            // only our own byte cap would read out of bounds.
            let allocation_bytes = GlobalSize(handle) as usize;
            if allocation_bytes < 2 {
                let _ = GlobalUnlock(handle);
                return Err(ClipboardError::Empty);
            }
            let max_units = (allocation_bytes / 2).min(MAX_CLIPBOARD_TEXT_BYTES / 2);
            let mut length = 0usize;
            while length < max_units && *pointer.add(length) != 0 {
                length += 1;
            }
            if length >= max_units {
                let _ = GlobalUnlock(handle);
                return Err(ClipboardError::TooLarge);
            }
            let units = std::slice::from_raw_parts(pointer, length);
            let text = String::from_utf16_lossy(units);
            let _ = GlobalUnlock(handle);
            if text.is_empty() {
                Err(ClipboardError::Empty)
            } else {
                Ok(text)
            }
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(not(windows))]
pub fn read_text() -> Result<String, ClipboardError> {
    Err(ClipboardError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_limit_is_bounded() {
        assert_eq!(MAX_CLIPBOARD_TEXT_BYTES, 4 * 1024 * 1024);
    }

    #[test]
    fn unsupported_platform_error_is_explicit() {
        #[cfg(not(windows))]
        assert_eq!(read_text(), Err(ClipboardError::UnsupportedPlatform));
    }
}

//! The add-on's WebAssembly module.
//!
//! The interface talks to it through one function, [`call`]: a request as
//! JSON and, with some requests, a file's bytes. The answer is JSON and,
//! with some, numbers for the 3D view or built meshes as bytes after it.
//! No binding generator is involved, so the module is the code here and
//! nothing else; the handful of lines of script that call it are in `ui/`.
//!
//! The module holds what it was given: the files of the user's BodySlide
//! folder that were handed to it, and the sets, presets, groups and
//! categories read from them.

mod character;
mod state;

pub use state::{handle, Answer};

use std::sync::Mutex;

static STATE: Mutex<Option<state::State>> = Mutex::new(None);

/// Room for `len` bytes, for the caller to fill and hand to [`call`].
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Give back what [`alloc`] or [`call`] handed out.
///
/// # Safety
/// `ptr` and `len` must be exactly what one of them returned, once.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
    }
}

/// Answer one request. The request is `request_len` bytes of JSON at
/// `request`, with `data_len` bytes at `data` when it carries a file.
///
/// The answer's address is in the high half of the result and its length in
/// the low half. It is four bytes giving the length of the JSON, the JSON,
/// and then any bytes the JSON describes. The caller gives it back with
/// [`dealloc`].
///
/// # Safety
/// Both ranges must be readable for their lengths.
#[no_mangle]
pub unsafe extern "C" fn call(
    request: *const u8,
    request_len: usize,
    data: *const u8,
    data_len: usize,
) -> u64 {
    let request = std::slice::from_raw_parts(request, request_len);
    let data = if data.is_null() || data_len == 0 {
        &[][..]
    } else {
        std::slice::from_raw_parts(data, data_len)
    };
    let answer = answer(request, data);
    let mut out = Vec::with_capacity(4 + answer.json.len() + answer.bytes.len());
    out.extend((answer.json.len() as u32).to_le_bytes());
    out.extend(answer.json.as_bytes());
    out.extend(&answer.bytes);
    out.shrink_to_fit();
    let (ptr, len) = (out.as_mut_ptr(), out.len());
    std::mem::forget(out);
    ((ptr as u64) << 32) | len as u64
}

/// [`call`], without the memory handling: what tests and a native build use.
pub fn answer(request: &[u8], data: &[u8]) -> Answer {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let state = guard.get_or_insert_with(state::State::default);
    handle(state, request, data)
}

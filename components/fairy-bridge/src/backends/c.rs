/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{ptr, slice, sync::Arc};

use crate::{
    init_backend, Backend, BackendSettings, FairyBridgeError, Method, Request, Response, Result,
};

const NULL: char = '\0';

/// Request for the backend
#[repr(C)]
pub struct FfiRequest {
    method: Method,
    url: *mut u8,
    headers: *mut FfiHeader,
    header_count: usize,
    body: *mut u8,
}

#[repr(C)]
pub struct FfiHeader {
    key: *mut u8,
    value: *mut u8,
}

/// Result from the backend
///
/// This is built-up piece by piece using the extern "C" API.
pub struct FfiResult {
    // oneshot sender that the Rust code is awaiting.  If `Ok(())` is sent, then the Rust code
    // should return the response.  If an error is sent, then that should be returned instead.
    sender: Option<oneshot::Sender<Result<()>>>,
    response: Response,
    // Owned values stored in the [FfiRequest].  These are copied from the request.  By storing
    // them in the result, we ensure they stay alive while the C code may access them.
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

// Function that the C / C++ library exports for us
extern "C" {
    fn fairy_bridge_backend_c_init(settings: BackendSettings);

    // (Rust flags this as an "improper C type", but the C code only uses it as an opaque pointer).
    #[allow(improper_ctypes)]
    fn fairy_bridge_backend_c_send_request(request: &mut FfiRequest, result: &mut FfiResult);
}

// Functions that we provide to the C / C++ library

/// Set the URL for a result
///
/// # Safety
///
/// - `result` must be valid.
/// - `url` and `length` must refer to a valid UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn fairy_bridge_result_set_url(
    result: &mut FfiResult,
    url: *mut u8,
    length: usize,
) {
    // Safety: this is safe as long as the backend passes us valid data
    result.response.url =
        unsafe { String::from_utf8_unchecked(slice::from_raw_parts_mut(url, length).to_vec()) };
}

/// Set the status code for a result
///
/// # Safety
///
/// `result` must be valid.
#[no_mangle]
pub unsafe extern "C" fn fairy_bridge_result_set_status_code(result: &mut FfiResult, code: u16) {
    result.response.status = code;
}

/// Set a header for a result
///
/// # Safety
///
/// - `result` must be valid.
/// - `key` and `key_length` must refer to a valid UTF-8 string.
/// - `value` and `value_length` must refer to a valid UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn fairy_bridge_result_add_header(
    result: &mut FfiResult,
    key: *mut u8,
    key_length: usize,
    value: *mut u8,
    value_length: usize,
) {
    // Safety: this is safe as long as the backend passes us valid data
    let (key, value) = unsafe {
        (
            String::from_utf8_unchecked(slice::from_raw_parts_mut(key, key_length).to_vec()),
            String::from_utf8_unchecked(slice::from_raw_parts_mut(value, value_length).to_vec()),
        )
    };
    result.response.headers.insert(key, value);
}

/// Append data to a result body
///
/// This method can be called multiple times to build up the body in chunks.
///
/// # Safety
///
/// - `result` must be valid.
/// - `data` and `length` must refer to a binary string.
#[no_mangle]
pub unsafe extern "C" fn fairy_bridge_result_extend_body(
    result: &mut FfiResult,
    data: *mut u8,
    length: usize,
) {
    // Safety: this is safe as long as the backend passes us valid data
    result
        .response
        .body
        .extend_from_slice(unsafe { slice::from_raw_parts_mut(data, length) });
}

/// Complete a result
///
/// # Safety
///
/// `result` must be valid.  After calling this function it must not be used again.
#[no_mangle]
pub unsafe extern "C" fn fairy_bridge_result_complete(result: &mut FfiResult) {
    match result.sender.take() {
        Some(sender) => {
            // Ignore any errors when sending the result.  This happens when the receiver is
            // closed, which happens when a future is cancelled.
            let _ = sender.send(Ok(()));
        }
        None => println!("fairy_bridge: result completed twice"),
    }
}

/// Complete a result with an error message
///
/// # Safety
///
/// - `result` must be valid.  After calling this function it must not be used again.
/// - `message` and `length` must refer to a valid UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn fairy_bridge_result_complete_error(
    result: &mut FfiResult,
    message: *mut u8,
    length: usize,
) {
    // Safety: this is safe as long as the backend passes us valid data
    let msg =
        unsafe { String::from_utf8_unchecked(slice::from_raw_parts_mut(message, length).to_vec()) };
    match result.sender.take() {
        Some(sender) => {
            // Ignore any errors when sending the result.  This happens when the receiver is
            // closed, which happens when a future is cancelled.
            let _ = sender.send(Err(FairyBridgeError::BackendError { msg }));
        }
        None => println!("fairy_bridge: result completed twice"),
    }
}

// The C-backend is a zero-sized type, since all the backend functionality is statically linked
struct CBackend;

#[uniffi::export]
fn init_backend_c(settings: BackendSettings) {
    // Safety: this is safe as long as the C code is correct.
    unsafe { fairy_bridge_backend_c_init(settings) };
    init_backend(Arc::new(CBackend)).expect("Error initializing C Backend");
}

#[async_trait::async_trait]
impl Backend for CBackend {
    async fn send_request(self: Arc<Self>, mut request: Request) -> Result<Response> {
        // Convert the request for the backend
        request.url.push(NULL);
        let mut header_list: Vec<_> = request.headers.into_iter().collect();
        for (key, value) in header_list.iter_mut() {
            key.push(NULL);
            value.push(NULL);
        }
        let mut ffi_headers: Vec<_> = header_list
            .iter_mut()
            .map(|(key, value)| FfiHeader {
                key: key.as_mut_ptr(),
                value: value.as_mut_ptr(),
            })
            .collect();
        let mut ffi_request = FfiRequest {
            method: request.method,
            url: request.url.as_mut_ptr(),
            headers: ffi_headers.as_mut_ptr(),
            header_count: ffi_headers.len(),
            body: match &mut request.body {
                Some(body) => body.as_mut_ptr(),
                None => ptr::null_mut(),
            },
        };

        // Prepare an FfiResult with an empty response
        let (sender, receiver) = oneshot::channel();
        let mut result = FfiResult {
            sender: Some(sender),
            response: Response::default(),
            url: request.url,
            headers: header_list,
            body: request.body,
        };

        // Safety: this is safe if the C backend implements the API correctly.
        unsafe {
            fairy_bridge_backend_c_send_request(&mut ffi_request, &mut result);
        };
        receiver
            .await
            .unwrap_or_else(|e| {
                Err(FairyBridgeError::BackendError {
                    msg: format!("Error receiving result: {e}"),
                })
            })
            .map(|_| result.response)
    }
}

// Mark FFI types as Send to allow them to be used across an await point.  This is safe as long as
// the backend code uses them correctly.
unsafe impl Send for FfiRequest {}
unsafe impl Send for FfiResult {}
unsafe impl Send for FfiHeader {}

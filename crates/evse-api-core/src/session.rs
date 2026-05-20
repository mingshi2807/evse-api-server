use std::ffi::{CStr, CString, c_char, c_void};
use tokio::sync::mpsc;

use crate::error::EvseApiError;
use crate::ffi;

pub struct Session {
    ptr: *mut ffi::Iso15118Session,
    _event_tx: mpsc::UnboundedSender<String>,
}

impl Session {
    pub fn new(config_json: &str) -> Result<(Self, mpsc::UnboundedReceiver<String>), EvseApiError> {
        let c_json = CString::new(config_json).map_err(|e| EvseApiError::Config(e.to_string()))?;
        let ptr = unsafe { ffi::iso15118_session_create(c_json.as_ptr()) };
        if ptr.is_null() {
            let err = unsafe { CStr::from_ptr(ffi::iso15118_last_error()) };
            return Err(EvseApiError::Iso15118(err.to_string_lossy().into_owned()));
        }

        let (event_tx, event_rx) = mpsc::unbounded_channel::<String>();

        let tx_box = Box::new(event_tx.clone());
        let userdata = Box::into_raw(tx_box) as *mut c_void;

        unsafe {
            ffi::iso15118_session_set_callback(ptr, session_event_callback, userdata);
        }

        Ok((
            Session {
                ptr,
                _event_tx: event_tx,
            },
            event_rx,
        ))
    }

    pub fn poll(&self) -> Option<u32> {
        let delay = unsafe { ffi::iso15118_session_poll(self.ptr) };
        if delay < 0 { None } else { Some(delay as u32) }
    }

    pub fn push_event(&self, event_json: &str) {
        let c_json = CString::new(event_json).unwrap();
        unsafe { ffi::iso15118_session_push_event(self.ptr, c_json.as_ptr()) };
    }

    pub fn close(&self) {
        unsafe { ffi::iso15118_session_close(self.ptr) };
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { ffi::iso15118_session_destroy(self.ptr) };
    }
}

unsafe impl Send for Session {}
unsafe impl Sync for Session {}

unsafe extern "C" fn session_event_callback(userdata: *mut c_void, json_event: *const c_char) {
    let tx: &mpsc::UnboundedSender<String> =
        unsafe { &*(userdata as *const mpsc::UnboundedSender<String>) };
    let event = unsafe { CStr::from_ptr(json_event) }
        .to_string_lossy()
        .into_owned();
    let _ = tx.send(event);
}

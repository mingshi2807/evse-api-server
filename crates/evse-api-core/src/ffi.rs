use std::ffi::{c_char, c_int, c_void};

#[repr(C)]
pub struct Iso15118Session {
    _private: [u8; 0],
}

pub type Iso15118EventFn =
    unsafe extern "C" fn(userdata: *mut c_void, json_event: *const c_char);

unsafe extern "C" {
    pub fn iso15118_session_create(config_json: *const c_char) -> *mut Iso15118Session;
    pub fn iso15118_session_destroy(session: *mut Iso15118Session);
    pub fn iso15118_session_poll(session: *mut Iso15118Session) -> c_int;
    pub fn iso15118_session_push_event(
        session: *mut Iso15118Session,
        event_json: *const c_char,
    );
    pub fn iso15118_session_set_callback(
        session: *mut Iso15118Session,
        fn_ptr: Iso15118EventFn,
        userdata: *mut c_void,
    );
    pub fn iso15118_session_close(session: *mut Iso15118Session);
    pub fn iso15118_last_error() -> *const c_char;
}

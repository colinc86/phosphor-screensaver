//! Password authentication module.
//!
//! When built with the `pam` feature (default), uses Linux PAM to verify
//! the current user's login password. PAM works for non-root processes on
//! most distros because pam_unix.so delegates to /sbin/unix_chkpwd.
//!
//! Without the `pam` feature, authentication always fails gracefully.

/// Get the current username from the environment.
pub fn current_username() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
}

/// Verify a password against the system.
/// Returns true if authentication succeeds.
#[cfg(feature = "pam")]
pub fn verify_password(username: &str, password: &str) -> bool {
    pam_auth::verify(username, password)
}

#[cfg(not(feature = "pam"))]
pub fn verify_password(_username: &str, _password: &str) -> bool {
    false
}

/// Check if password authentication is available.
/// Check if password authentication is available.
#[allow(dead_code)]
pub fn is_available() -> bool {
    cfg!(feature = "pam")
}

#[cfg(feature = "pam")]
mod pam_auth {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_void};
    use std::ptr;

    const PAM_SUCCESS: c_int = 0;
    const PAM_PROMPT_ECHO_OFF: c_int = 1;

    type PamHandle = c_void;

    #[repr(C)]
    struct PamMessage {
        msg_style: c_int,
        _msg: *const c_char,
    }

    #[repr(C)]
    struct PamResponse {
        resp: *mut c_char,
        resp_retcode: c_int,
    }

    type ConvFn = unsafe extern "C" fn(
        c_int,
        *mut *const PamMessage,
        *mut *mut PamResponse,
        *mut c_void,
    ) -> c_int;

    #[repr(C)]
    struct PamConv {
        conv: Option<ConvFn>,
        appdata_ptr: *mut c_void,
    }

    #[link(name = "pam")]
    unsafe extern "C" {
        fn pam_start(
            service_name: *const c_char,
            user: *const c_char,
            pam_conversation: *const PamConv,
            pamh: *mut *mut PamHandle,
        ) -> c_int;
        fn pam_authenticate(pamh: *mut PamHandle, flags: c_int) -> c_int;
        fn pam_end(pamh: *mut PamHandle, pam_status: c_int) -> c_int;
    }

    unsafe extern "C" fn conversation(
        num_msg: c_int,
        msg: *mut *const PamMessage,
        resp: *mut *mut PamResponse,
        appdata_ptr: *mut c_void,
    ) -> c_int {
        let password = unsafe { &*(appdata_ptr as *const CString) };
        let responses = unsafe {
            libc::calloc(num_msg as usize, std::mem::size_of::<PamResponse>()) as *mut PamResponse
        };
        if responses.is_null() {
            return 5; // PAM_BUF_ERR
        }

        for i in 0..num_msg as isize {
            let message = unsafe { *msg.offset(i) };
            if unsafe { (*message).msg_style } == PAM_PROMPT_ECHO_OFF {
                unsafe {
                    (*responses.offset(i)).resp = libc::strdup(password.as_ptr());
                    (*responses.offset(i)).resp_retcode = 0;
                }
            }
        }

        unsafe { *resp = responses };
        PAM_SUCCESS
    }

    pub fn verify(username: &str, password: &str) -> bool {
        let service = match CString::new("login") {
            Ok(s) => s,
            Err(_) => return false,
        };
        let user = match CString::new(username) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let pass = match CString::new(password) {
            Ok(s) => s,
            Err(_) => return false,
        };

        let conv = PamConv {
            conv: Some(conversation),
            appdata_ptr: &pass as *const CString as *mut c_void,
        };

        let mut pamh: *mut PamHandle = ptr::null_mut();
        let ret = unsafe { pam_start(service.as_ptr(), user.as_ptr(), &conv, &mut pamh) };
        if ret != PAM_SUCCESS {
            return false;
        }

        let auth_ret = unsafe { pam_authenticate(pamh, 0) };
        unsafe { pam_end(pamh, auth_ret) };

        auth_ret == PAM_SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_username_returns_something() {
        let user = current_username();
        assert!(user.is_some());
        assert!(!user.unwrap().is_empty());
    }

    #[cfg(feature = "pam")]
    #[test]
    fn wrong_password_fails() {
        if let Some(user) = current_username() {
            let result = verify_password(&user, "definitely_wrong_password_12345");
            assert!(!result);
        }
    }
}

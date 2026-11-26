//! The network link for online play: in the browser, a Supabase Realtime room through a
//! small JS plugin (web/net.js); natively, not available (yet).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub enum Status {
    Off,
    Joining,
    /// In the room (whoever else is there or not).
    Joined,
    Error,
    /// This build has no online project configured, or runs natively.
    Unavailable,
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::Status;

    unsafe extern "C" {
        fn bb_net_join(ptr: *const u8, len: u32);
        fn bb_net_send(ptr: *const u8, len: u32);
        fn bb_net_poll() -> i32;
        fn bb_net_take(ptr: *mut u8);
        fn bb_net_status() -> i32;
        fn bb_net_peers() -> i32;
        fn bb_net_is_host() -> i32;
        fn bb_net_leave();
    }

    pub fn join(room: &str) {
        unsafe { bb_net_join(room.as_ptr(), room.len() as u32) }
    }
    pub fn send(text: &str) {
        unsafe { bb_net_send(text.as_ptr(), text.len() as u32) }
    }
    pub fn poll() -> Option<String> {
        let n = unsafe { bb_net_poll() };
        if n < 0 {
            return None;
        }
        let mut buf = vec![0u8; n as usize];
        unsafe { bb_net_take(buf.as_mut_ptr()) };
        String::from_utf8(buf).ok()
    }
    pub fn status() -> Status {
        match unsafe { bb_net_status() } {
            1 => Status::Joining,
            2 => Status::Joined,
            3 => Status::Error,
            4 => Status::Unavailable,
            _ => Status::Off,
        }
    }
    pub fn peers() -> usize {
        unsafe { bb_net_peers() }.max(0) as usize
    }
    pub fn is_host() -> bool {
        unsafe { bb_net_is_host() == 1 }
    }
    pub fn leave() {
        unsafe { bb_net_leave() }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::Status;
    pub fn join(_: &str) {}
    pub fn send(_: &str) {}
    pub fn poll() -> Option<String> {
        None
    }
    pub fn status() -> Status {
        Status::Unavailable
    }
    pub fn peers() -> usize {
        0
    }
    pub fn is_host() -> bool {
        false
    }
    pub fn leave() {}
}

pub use imp::{is_host, join, leave, peers, poll, send, status};

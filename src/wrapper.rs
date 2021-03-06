#![unstable]
#![allow(missing_docs)]

//! Idiomatic wrapper for inotify

use libc::{
    EAGAIN,
    EWOULDBLOCK,
    F_GETFL,
    F_SETFL,
    O_NONBLOCK,
    fcntl,
    c_int,
    c_void,
    size_t,
    ssize_t
};
use std::ffi::{
    CString,
};
use std::mem;
use std::io::{
    EndOfFile,
    IoError,
    IoResult
};
use std::os::errno;
use std::path::BytesContainer;
use std::slice;

use ffi;
use ffi::inotify_event;


pub type Watch = c_int;

#[derive(Clone)]
pub struct INotify {
    pub fd: c_int,
    events: Vec<Event>,
}

impl INotify {
    pub fn init() -> IoResult<INotify> {
        INotify::init_with_flags(0)
    }

    pub fn init_with_flags(flags: isize) -> IoResult<INotify> {
        let fd = unsafe { ffi::inotify_init1(flags as c_int) };

        unsafe { fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK) };

        match fd {
            -1 => Err(IoError::last_error()),
            _  => Ok(INotify {
                fd    : fd,
                events: Vec::new(),
            })
        }
    }

    pub fn add_watch(&self, path: &Path, mask: u32) -> IoResult<Watch> {
        let wd = unsafe {
            let path_c_str = CString::from_slice(path.container_as_bytes());
            ffi::inotify_add_watch(
                self.fd,
                path_c_str.as_ptr(),
                mask)
        };

        match wd {
            -1 => Err(IoError::last_error()),
            _  => Ok(wd)
        }
    }

    pub fn rm_watch(&self, watch: Watch) -> IoResult<()> {
        let result = unsafe { ffi::inotify_rm_watch(self.fd, watch) };
        match result {
            0  => Ok(()),
            -1 => Err(IoError::last_error()),
            _  => panic!(
                "unexpected return code from inotify_rm_watch ({})", result)
        }
    }

    /// Wait until events are available, then return them.
    /// This function will block until events are available. If you want it to
    /// return immediately, use `available_events`.
    pub fn wait_for_events(&mut self) -> IoResult<&[Event]> {
        let fd = self.fd;

        unsafe {
            fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) & !O_NONBLOCK)
        };
        let result = self.available_events();
        unsafe {
            fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK)
        };

        result
    }

    /// Returns available inotify events.
    /// If no events are available, this method will simply return a slice with
    /// zero events. If you want to wait for events to become available, call
    /// `wait_for_events`.
    pub fn available_events(&mut self) -> IoResult<&[Event]> {
        self.events.clear();

        let mut buffer = [0u8; 1024];
        let len = unsafe {
            ffi::read(
                self.fd,
                buffer.as_mut_ptr() as *mut c_void,
                buffer.len() as size_t
            )
        };

        match len {
            0 =>
                return Err(IoError {
                    kind  : EndOfFile,
                    desc  : "end of file",
                    detail: None
                }),
            -1 => {
                let error = errno();
                if error == EAGAIN as usize || error == EWOULDBLOCK as usize {
                    return Ok(&self.events[]);
                }
                else {
                    return Err(IoError::from_errno(error, true));
                }
            },
            _ =>
                ()
        }

        let event_size = mem::size_of::<inotify_event>();

        let mut i = 0;
        while i < len {
            unsafe {
                let slice = &buffer[i as usize..];

                let event = slice.as_ptr() as *const inotify_event;

                let name = if (*event).len > 0 {
                    let name_ptr = slice
                        .as_ptr()
                        .offset(event_size as isize);

                    let mut name_slice = slice::from_raw_buf(
                        &name_ptr,
                        (*event).len as usize,
                    );

                    // Make sure slice contains no \0, CString doesn't like
                    // them.
                    let pos = name_slice.position_elem(&0);
                    match pos {
                        Some(pos) => name_slice = &name_slice[..pos],
                        None      => (),
                    }

                    let c_str = CString::from_slice(name_slice);

                    match c_str.container_as_str() {
                        Some(string)
                            => string.to_string(),
                        None =>
                            panic!("Failed to convert C string into Rust string")
                    }
                }
                else {
                    "".to_string()
                };

                self.events.push(Event::new(&*event, name));

                i += (event_size + (*event).len as usize) as ssize_t;
            }
        }

        Ok(&self.events[])
    }

    pub fn close(&self) -> IoResult<()> {
        let result = unsafe { ffi::close(self.fd) };
        match result {
            0 => Ok(()),
            _ => Err(IoError::last_error())
        }
    }
}

#[derive(Clone, Show)]
pub struct Event {
    pub wd    : i32,
    pub mask  : u32,
    pub cookie: u32,
    pub name  : String,
}

impl Event {
    fn new(event: &inotify_event, name: String) -> Event {
        Event {
            wd    : event.wd,
            mask  : event.mask,
            cookie: event.cookie,
            name  : name,
        }
    }

    pub fn is_access(&self) -> bool {
        return self.mask & ffi::IN_ACCESS > 0;
    }

    pub fn is_modify(&self) -> bool {
        return self.mask & ffi::IN_MODIFY > 0;
    }

    pub fn is_attrib(&self) -> bool {
        return self.mask & ffi::IN_ATTRIB > 0;
    }

    pub fn is_close_write(&self) -> bool {
        return self.mask & ffi::IN_CLOSE_WRITE > 0;
    }

    pub fn is_close_nowrite(&self) -> bool {
        return self.mask & ffi::IN_CLOSE_NOWRITE > 0;
    }

    pub fn is_open(&self) -> bool {
        return self.mask & ffi::IN_OPEN > 0;
    }

    pub fn is_moved_from(&self) -> bool {
        return self.mask & ffi::IN_MOVED_FROM > 0;
    }

    pub fn is_moved_to(&self) -> bool {
        return self.mask & ffi::IN_MOVED_TO > 0;
    }

    pub fn is_create(&self) -> bool {
        return self.mask & ffi::IN_CREATE > 0;
    }

    pub fn is_delete(&self) -> bool {
        return self.mask & ffi::IN_DELETE > 0;
    }

    pub fn is_delete_self(&self) -> bool {
        return self.mask & ffi::IN_DELETE_SELF > 0;
    }

    pub fn is_move_self(&self) -> bool {
        return self.mask & ffi::IN_MOVE_SELF > 0;
    }

    pub fn is_move(&self) -> bool {
        return self.mask & ffi::IN_MOVE > 0;
    }

    pub fn is_close(&self) -> bool {
        return self.mask & ffi::IN_CLOSE > 0;
    }

    pub fn is_dir(&self) -> bool {
        return self.mask & ffi::IN_ISDIR > 0;
    }

    pub fn is_unmount(&self) -> bool {
        return self.mask & ffi::IN_UNMOUNT > 0;
    }

    pub fn is_queue_overflow(&self) -> bool {
        return self.mask & ffi::IN_Q_OVERFLOW > 0;
    }

    pub fn is_ignored(&self) -> bool {
        return self.mask & ffi::IN_IGNORED > 0;
    }
}

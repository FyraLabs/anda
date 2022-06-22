
use libsolv_bind::Chksum as _Chksum;
use libsolv_bind::Id;

use std::ptr;
use std::mem;
use libsolv_bind::solv_knownid;
use std::slice;
use libc;
use std::fs::File;
use std::io::{Cursor, Read, BufReader};
use std::os::unix::io::*;
use std::convert::Into;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checksum {
    _c: *mut _Chksum,
}

impl Checksum {
    fn new(id: Id) -> Checksum {
        use libsolv_bind::solv_chksum_create;
        let _c = unsafe{solv_chksum_create(id)};
        if _c.is_null() {
            panic!("libsolv returned null for solv_chksum_create(Id) with id {}", id);
        } else {
            Checksum{_c: _c}
        }
    }

    pub(crate) unsafe fn new_from(_c: *mut _Chksum) -> Checksum {
        if _c.is_null() {
            panic!("libsolv returned null for solv_chksum_create(Id)");
        }
        Checksum{_c: _c}
    }

    pub fn new_md5() -> Checksum {
        Checksum::new(libsolv_bind::solv_knownid_REPOKEY_TYPE_MD5 as Id)
    }

    pub fn new_sha1() -> Checksum {
        Checksum::new(libsolv_bind::solv_knownid_REPOKEY_TYPE_SHA1 as Id)
    }

    pub fn new_sha256() -> Checksum {
        Checksum::new(libsolv_bind::solv_knownid_REPOKEY_TYPE_SHA256 as Id)
    }

    pub fn add<T: AsRef<[u8]>>(&mut self, t: T) {
        let mut c = Cursor::new(t.as_ref());
        self.read_in(&mut c);
    }

    pub fn read_in<R: Read>(&mut self, r: &mut R) {
        use libsolv_bind::solv_chksum_add;
        let mut buffer: [u8; 4096] = [0; 4096];

        let mut reader = BufReader::new(r);
        while let Ok(l) = reader.read(&mut buffer) {
            if l == 0 {
                break;
            }
            unsafe {solv_chksum_add(self._c, buffer.as_ptr() as *const libc::c_void, l as i32)};
        }
    }

    pub fn add_fstat(&mut self, file: &File) {
        use libsolv_bind::solv_chksum_add;
        let stb: libc::stat = unsafe {
            let mut tmp = mem::MaybeUninit::uninit().assume_init();
            if libc::fstat(file.as_raw_fd(), &mut tmp) == 0 {
                mem::MaybeUninit::uninit().assume_init()
            } else {
                tmp
            }
        };
        unsafe {
            solv_chksum_add(self._c, &stb.st_dev as *const libc::dev_t as *const libc::c_void, mem::size_of::<libc::dev_t>() as i32);
            solv_chksum_add(self._c, &stb.st_ino as *const libc::ino_t as *const libc::c_void, mem::size_of::<libc::ino_t>() as i32);
            solv_chksum_add(self._c, &stb.st_size as *const libc::off_t as *const libc::c_void, mem::size_of::<libc::off_t>() as i32);
            solv_chksum_add(self._c, &stb.st_mtime as *const libc::time_t as *const libc::c_void, mem::size_of::<libc::time_t>() as i32);
        }
    }

    pub fn into_boxed_slice(self) -> Box<[u8]> {
        use libsolv_bind::solv_chksum_get;
        let mut l = 0;
        let slice = unsafe {
            let ptr = solv_chksum_get(self._c, &mut l);
            slice::from_raw_parts(ptr, l as usize)
        };
        Vec::from(slice).into_boxed_slice()
    }
}

impl Into<Box<[u8]>> for Checksum {
    fn into(self) -> Box<[u8]> {
        self.into_boxed_slice()
    }
}

impl Drop for Checksum {
    fn drop(&mut self) {
        use libsolv_bind::solv_chksum_free;
        unsafe {solv_chksum_free(self._c, ptr::null_mut() as *mut u8)};
    }
}
// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Interfaces to the operating system provided random number
//! generators.

pub use self::imp::OSRng;

#[cfg(unix)]
mod imp {
    use Rng;
    use reader::ReaderRng;
    use std::io::File;

    /// A random number generator that retrieves randomness straight from
    /// the operating system. Platform sources:
    ///
    /// - Unix-like systems (Linux, Android, Mac OSX): read directly from
    ///   `/dev/urandom`.
    /// - Windows: calls `CryptGenRandom`, using the default cryptographic
    ///   service provider with the `PROV_RSA_FULL` type.
    ///
    /// This does not block.
    #[cfg(unix)]
    pub struct OSRng {
        priv inner: ReaderRng<File>
    }

    impl OSRng {
        /// Create a new `OSRng`.
        pub fn new() -> OSRng {
            let reader = File::open(&Path::new("/dev/urandom"));
            let reader = reader.ok().expect("Error opening /dev/urandom");
            let reader_rng = ReaderRng::new(reader);

            OSRng { inner: reader_rng }
        }
    }

    impl Rng for OSRng {
        fn next_u32(&mut self) -> u32 {
            self.inner.next_u32()
        }
        fn next_u64(&mut self) -> u64 {
            self.inner.next_u64()
        }
        fn fill_bytes(&mut self, v: &mut [u8]) {
            self.inner.fill_bytes(v)
        }
    }
}

#[cfg(windows)]
mod imp {
    use Rng;
    use std::cast;
    use std::libc::{c_ulong, DWORD, BYTE, LPCSTR, BOOL};
    use std::os;

    type HCRYPTPROV = c_ulong;

    /// A random number generator that retrieves randomness straight from
    /// the operating system. Platform sources:
    ///
    /// - Unix-like systems (Linux, Android, Mac OSX): read directly from
    ///   `/dev/urandom`.
    /// - Windows: calls `CryptGenRandom`, using the default cryptographic
    ///   service provider with the `PROV_RSA_FULL` type.
    ///
    /// This does not block.
    pub struct OSRng {
        priv hcryptprov: HCRYPTPROV
    }

    static PROV_RSA_FULL: DWORD = 1;
    static CRYPT_SILENT: DWORD = 64;
    static CRYPT_VERIFYCONTEXT: DWORD = 0xF0000000;

    extern "system" {
        fn CryptAcquireContextA(phProv: *mut HCRYPTPROV,
                                pszContainer: LPCSTR,
                                pszProvider: LPCSTR,
                                dwProvType: DWORD,
                                dwFlags: DWORD) -> BOOL;
        fn CryptGenRandom(hProv: HCRYPTPROV,
                          dwLen: DWORD,
                          pbBuffer: *mut BYTE) -> BOOL;
        fn CryptReleaseContext(hProv: HCRYPTPROV, dwFlags: DWORD) -> BOOL;
    }

    impl OSRng {
        /// Create a new `OSRng`.
        pub fn new() -> OSRng {
            let mut hcp = 0;
            let ret = unsafe {
                CryptAcquireContextA(&mut hcp, 0 as LPCSTR, 0 as LPCSTR,
                                     PROV_RSA_FULL,
                                     CRYPT_VERIFYCONTEXT | CRYPT_SILENT)
            };
            if ret == 0 {
                fail!("couldn't create context: {}", os::last_os_error());
            }
            OSRng { hcryptprov: hcp }
        }
    }

    impl Rng for OSRng {
        fn next_u32(&mut self) -> u32 {
            let mut v = [0u8, .. 4];
            self.fill_bytes(v);
            unsafe { cast::transmute(v) }
        }
        fn next_u64(&mut self) -> u64 {
            let mut v = [0u8, .. 8];
            self.fill_bytes(v);
            unsafe { cast::transmute(v) }
        }
        fn fill_bytes(&mut self, v: &mut [u8]) {
            let ret = unsafe {
                CryptGenRandom(self.hcryptprov, v.len() as DWORD,
                               v.as_mut_ptr())
            };
            if ret == 0 {
                fail!("couldn't generate random bytes: {}", os::last_os_error());
            }
        }
    }

    impl Drop for OSRng {
        fn drop(&mut self) {
            let ret = unsafe {
                CryptReleaseContext(self.hcryptprov, 0)
            };
            if ret == 0 {
                fail!("couldn't release context: {}", os::last_os_error());
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::OSRng;
    use Rng;
    use std::task;

    #[test]
    fn test_os_rng() {
        let mut r = OSRng::new();

        r.next_u32();
        r.next_u64();

        let mut v = [0u8, .. 1000];
        r.fill_bytes(v);
    }

    #[test]
    fn test_os_rng_tasks() {

        let mut txs = ~[];
        for _ in range(0, 20) {
            let (tx, rx) = channel();
            txs.push(tx);
            task::spawn(proc() {
                // wait until all the tasks are ready to go.
                rx.recv();

                // deschedule to attempt to interleave things as much
                // as possible (XXX: is this a good test?)
                let mut r = OSRng::new();
                task::deschedule();
                let mut v = [0u8, .. 1000];

                for _ in range(0, 100) {
                    r.next_u32();
                    task::deschedule();
                    r.next_u64();
                    task::deschedule();
                    r.fill_bytes(v);
                    task::deschedule();
                }
            })
        }

        // start all the tasks
        for tx in txs.iter() {
            tx.send(())
        }
    }
}
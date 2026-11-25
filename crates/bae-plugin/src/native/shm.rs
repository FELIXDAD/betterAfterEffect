//! Shared frame storage. The host owns the segment for its whole lifetime.
//! Only one side accesses the pixels at a time, across a request/reply boundary.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ShmSpec {
    len: usize,
    #[cfg(windows)]
    name: String,
}

pub(super) use platform::{Mapping, Shm};

#[cfg(unix)]
mod platform {
    use std::fs::File;
    use std::io::{self, IoSlice, IoSliceMut};
    use std::mem::MaybeUninit;
    use std::os::fd::AsFd;
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::{AtomicU32, Ordering};

    use rustix::net::{
        RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, SendAncillaryBuffer,
        SendAncillaryMessage, SendFlags, recvmsg, sendmsg,
    };
    use rustix::shm::{self, Mode, OFlags};

    use super::ShmSpec;
    pub(crate) type Mapping = memmap2::MmapMut;

    pub(crate) struct Shm {
        file: File,
        pub(crate) map: Mapping,
    }

    impl Shm {
        pub(crate) fn create(len: usize) -> io::Result<Self> {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let name = format!(
                "/bae-{:x}-{:x}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let fd = shm::open(
                &*name,
                OFlags::CREATE | OFlags::EXCL | OFlags::RDWR,
                Mode::RUSR | Mode::WUSR,
            )?;
            shm::unlink(&*name)?;
            let file = File::from(fd);
            file.set_len(len as u64)?;
            // SAFETY: only this pair can access the descriptor. Its size is fixed.
            let map = unsafe { Mapping::map_mut(&file) }?;
            Ok(Self { file, map })
        }

        pub(crate) fn spec(&self) -> ShmSpec {
            ShmSpec {
                len: self.map.len(),
            }
        }

        /// Send the descriptor just after the Render message on the stdin socket.
        pub(crate) fn send_fd(&self, socket: &UnixStream) -> io::Result<()> {
            let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
            let fds = [self.file.as_fd()];
            let mut control = SendAncillaryBuffer::new(&mut space);
            let added = control.push(SendAncillaryMessage::ScmRights(&fds));
            debug_assert!(added);
            sendmsg(
                socket,
                &[IoSlice::new(&[0])],
                &mut control,
                SendFlags::empty(),
            )?;
            Ok(())
        }
    }

    impl ShmSpec {
        pub(crate) fn open(&self) -> io::Result<Mapping> {
            let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
            let mut control = RecvAncillaryBuffer::new(&mut space);
            recvmsg(
                io::stdin(),
                &mut [IoSliceMut::new(&mut [0])],
                &mut control,
                RecvFlags::empty(),
            )?;
            let file = control
                .drain()
                .find_map(|msg| match msg {
                    RecvAncillaryMessage::ScmRights(mut rights) => rights.next().map(File::from),
                    _ => None,
                })
                .ok_or_else(|| io::Error::other("missing shared memory descriptor"))?;
            if file.metadata()?.len() != self.len as u64 {
                return Err(io::Error::other("shared memory length mismatch"));
            }
            // SAFETY: the host sized this object once. It waits for our reply before reuse.
            unsafe { Mapping::map_mut(&file) }
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::io;
    use std::ops::{Deref, DerefMut};
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::ptr::NonNull;

    use windows_sys::Win32::Foundation::{
        ERROR_ALREADY_EXISTS, GetLastError, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
        OpenFileMappingW, PAGE_READWRITE, UnmapViewOfFile,
    };

    use super::ShmSpec;

    pub(crate) struct Mapping {
        _section: OwnedHandle,
        base: NonNull<u8>,
        len: usize,
    }

    // SAFETY: the view has no thread affinity. The runner client's mutex and
    // the request/reply protocol serialize access to it.
    unsafe impl Send for Mapping {}

    impl Mapping {
        fn view(section: OwnedHandle, len: usize) -> io::Result<Self> {
            // SAFETY: section is live, and a successful view covers len bytes.
            let addr = unsafe { MapViewOfFile(section.as_raw_handle(), FILE_MAP_WRITE, 0, 0, len) };
            let base = NonNull::new(addr.Value.cast()).ok_or_else(io::Error::last_os_error)?;
            Ok(Self {
                _section: section,
                base,
                len,
            })
        }
    }

    impl Deref for Mapping {
        type Target = [u8];
        fn deref(&self) -> &[u8] {
            // SAFETY: this view remains mapped until Drop. See module access protocol.
            unsafe { std::slice::from_raw_parts(self.base.as_ptr(), self.len) }
        }
    }

    impl DerefMut for Mapping {
        fn deref_mut(&mut self) -> &mut [u8] {
            // SAFETY: the caller holds exclusive access during this protocol phase.
            unsafe { std::slice::from_raw_parts_mut(self.base.as_ptr(), self.len) }
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: this object owns the view returned by MapViewOfFile.
            unsafe {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.base.as_ptr().cast(),
                })
            };
        }
    }

    pub(crate) struct Shm {
        name: String,
        pub(crate) map: Mapping,
    }

    fn wide(name: &str) -> Vec<u16> {
        name.encode_utf16().chain(Some(0)).collect()
    }

    impl Shm {
        pub(crate) fn create(len: usize) -> io::Result<Self> {
            let name = format!(r"Local\betterAfterEffects-{}", uuid::Uuid::new_v4());
            // SAFETY: a pagefile-backed section, default security, live UTF-16 name.
            let wide_name = wide(&name);
            let raw = unsafe {
                CreateFileMappingW(
                    INVALID_HANDLE_VALUE,
                    std::ptr::null(),
                    PAGE_READWRITE,
                    ((len as u64) >> 32) as u32,
                    len as u32,
                    wide_name.as_ptr(),
                )
            };
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: successful CreateFileMappingW gives us an owned handle.
            let section = unsafe { OwnedHandle::from_raw_handle(raw) };
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            Ok(Self {
                name,
                map: Mapping::view(section, len)?,
            })
        }

        pub(crate) fn spec(&self) -> ShmSpec {
            ShmSpec {
                name: self.name.clone(),
                len: self.map.len(),
            }
        }
    }

    impl ShmSpec {
        pub(crate) fn open(&self) -> io::Result<Mapping> {
            // SAFETY: the name is NUL-terminated and valid for the call.
            let wide_name = wide(&self.name);
            let raw = unsafe { OpenFileMappingW(FILE_MAP_WRITE, 0, wide_name.as_ptr()) };
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: successful OpenFileMappingW gives us an owned handle.
            Mapping::view(unsafe { OwnedHandle::from_raw_handle(raw) }, self.len)
        }
    }
}

//! Optional Media Foundation entry points. Windows N can run the rest of BAE
//! without the Media Feature Pack, including before the first movie is opened.

use std::path::Path;

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::*;
use windows::core::{GUID, HRESULT, HSTRING, Interface, Owned, s, w};

use crate::{FrameRate, MediaError};

impl From<windows::core::Error> for MediaError {
    fn from(error: windows::core::Error) -> Self {
        Self::Platform {
            framework: "Media Foundation",
            message: error.to_string(),
        }
    }
}

type Out<T> = *mut Option<T>;
type Create<T> = unsafe extern "system" fn(Out<T>) -> HRESULT;
type Startup = unsafe extern "system" fn(u32, u32) -> HRESULT;
type Shutdown = unsafe extern "system" fn() -> HRESULT;
type Attributes = unsafe extern "system" fn(Out<IMFAttributes>, u32) -> HRESULT;
type Manager = unsafe extern "system" fn(*mut u32, Out<IMFDXGIDeviceManager>) -> HRESULT;
type Buffer = unsafe extern "system" fn(u32, Out<IMFMediaBuffer>) -> HRESULT;
type Enumerate = unsafe extern "system" fn(
    GUID,
    MFT_ENUM_FLAG,
    *const MFT_REGISTER_TYPE_INFO,
    *const MFT_REGISTER_TYPE_INFO,
    *mut Out<IMFActivate>,
    *mut u32,
) -> HRESULT;

/// The MF reference must outlive the reader/writer and all of its samples.
pub(super) struct Platform {
    factory: IMFReadWriteClassFactory,
    _dll: Owned<HMODULE>,
    _mta: Owned<CO_MTA_USAGE_COOKIE>,
    shutdown: Shutdown,
    attributes: Attributes,
    media_type: Create<IMFMediaType>,
    manager: Manager,
    pub sample: Create<IMFSample>,
    pub buffer: Buffer,
    pub enumerate: Enumerate,
}

impl Platform {
    pub fn new() -> Result<Self, MediaError> {
        // SAFETY: load from System32. The signatures match mfapi.h; every
        // interface out parameter is a pointer to a nullable COM reference.
        unsafe {
            let dll = Owned::new(
                LoadLibraryExW(w!("mfplat.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).map_err(
                    |e| {
                        MediaError::Unsupported(format!(
                            "Windows Media Feature Pack (mfplat.dll): {e}"
                        ))
                    },
                )?,
            );
            macro_rules! entry {
                ($name:literal, $ty:ty) => {{
                    let address = GetProcAddress(*dll, s!($name))
                        .ok_or_else(windows::core::Error::from_thread)?;
                    std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(address)
                }};
            }
            let startup = entry!("MFStartup", Startup);
            let shutdown = entry!("MFShutdown", Shutdown);
            let attributes = entry!("MFCreateAttributes", Attributes);
            let media_type = entry!("MFCreateMediaType", Create<IMFMediaType>);
            let manager = entry!("MFCreateDXGIDeviceManager", Manager);
            let sample = entry!("MFCreateSample", Create<IMFSample>);
            let buffer = entry!("MFCreateMemoryBuffer", Buffer);
            let enumerate = entry!("MFTEnumEx", Enumerate);
            let mta = Owned::new(CoIncrementMTAUsage()?);
            let factory =
                CoCreateInstance(&CLSID_MFReadWriteClassFactory, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| {
                        MediaError::Unsupported(format!(
                            "Windows Media Foundation Read/Write component: {e}"
                        ))
                    })?;
            startup(MF_VERSION, MFSTARTUP_FULL).ok()?;
            Ok(Self {
                factory,
                _dll: dll,
                _mta: mta,
                shutdown,
                attributes,
                media_type,
                manager,
                sample,
                buffer,
                enumerate,
            })
        }
    }

    pub fn attributes(&self) -> windows::core::Result<IMFAttributes> {
        let mut attrs = None;
        // SAFETY: valid COM out parameter.
        unsafe { (self.attributes)(&mut attrs, 4).ok()? };
        Ok(attrs.unwrap())
    }

    pub fn video_type(&self, subtype: &GUID) -> windows::core::Result<IMFMediaType> {
        self.stream_type(&MFMediaType_Video, subtype)
    }

    pub fn stream_type(&self, major: &GUID, subtype: &GUID) -> windows::core::Result<IMFMediaType> {
        // SAFETY: valid out parameter and MF attribute keys.
        unsafe {
            let mut ty = None;
            (self.media_type)(&mut ty).ok()?;
            let ty = ty.unwrap();
            ty.SetGUID(&MF_MT_MAJOR_TYPE, major)?;
            ty.SetGUID(&MF_MT_SUBTYPE, subtype)?;
            Ok(ty)
        }
    }

    pub fn device_manager(&self) -> windows::core::Result<IMFDXGIDeviceManager> {
        // SAFETY: hardware device on the default adapter, retained by the DXGI manager.
        unsafe {
            let device = video_device()?;
            let mut token = 0;
            let mut manager = None;
            (self.manager)(&mut token, &mut manager).ok()?;
            let manager = manager.unwrap();
            manager.ResetDevice(&device, token)?;
            Ok(manager)
        }
    }

    pub fn warm_decoder(&self, subtype: GUID, hardware: bool) -> windows::core::Result<()> {
        let input = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: subtype,
        };
        let mut flags =
            MFT_ENUM_FLAG_SORTANDFILTER | MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_ASYNCMFT;
        if hardware {
            flags |= MFT_ENUM_FLAG_HARDWARE;
        }
        let mut list = std::ptr::null_mut();
        let mut count = 0;
        // SAFETY: MFTEnumEx gives us an array of owned activation objects. The
        // preferred available decoder is activated on the warm-up MTA thread.
        unsafe {
            (self.enumerate)(
                MFT_CATEGORY_VIDEO_DECODER,
                flags,
                &input,
                std::ptr::null(),
                &mut list,
                &mut count,
            )
            .ok()?;
            if !list.is_null() {
                let items = std::ptr::slice_from_raw_parts_mut(list, count as usize);
                let result = (&*items).first().and_then(Option::as_ref).map(|decoder| {
                    let transform: IMFTransform = decoder.ActivateObject()?;
                    let _ = transform.GetInputAvailableType(0, 0);
                    decoder.ShutdownObject()
                });
                std::ptr::drop_in_place(items);
                CoTaskMemFree(Some(list.cast()));
                result.transpose()?;
            }
        }
        Ok(())
    }

    pub fn open_url<T: Interface>(
        &self,
        class: &GUID,
        path: &Path,
        attrs: &IMFAttributes,
    ) -> windows::core::Result<T> {
        // SAFETY: the process holds an MTA reference and the factory returns T by IID.
        unsafe {
            self.factory
                .CreateInstanceFromURL(class, &HSTRING::from(path.as_os_str()), attrs)
        }
    }
}

pub(super) fn video_device() -> windows::core::Result<ID3D11Device> {
    static DEVICE: std::sync::Mutex<Option<ID3D11Device>> = std::sync::Mutex::new(None);
    let mut cached = DEVICE.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: GetDeviceRemovedReason only queries the current device.
    if let Some(device) = cached.as_ref()
        && unsafe { device.GetDeviceRemovedReason() }.is_ok()
    {
        return Ok(device.clone());
    }
    // SAFETY: request the default hardware adapter and protect its immediate
    // context before passing it to Media Foundation's worker threads.
    unsafe {
        let mut device = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )?;
        let device = device.unwrap();
        let _ = device
            .cast::<ID3D11Multithread>()?
            .SetMultithreadProtected(true);
        *cached = Some(device.clone());
        Ok(device)
    }
}

impl Drop for Platform {
    fn drop(&mut self) {
        // SAFETY: balances this instance's successful startup before unloading the DLL.
        unsafe {
            let _ = (self.shutdown)();
        }
    }
}

pub(super) fn timestamp(index: i64, rate: FrameRate) -> i64 {
    (i128::from(index) * 10_000_000 * i128::from(rate.den) / i128::from(rate.num)) as i64
}

pub(super) fn frame_index(time: i64, rate: FrameRate) -> i64 {
    let scale = 10_000_000 * i128::from(rate.den);
    ((i128::from(time) * i128::from(rate.num) + scale / 2).div_euclid(scale)) as i64
}

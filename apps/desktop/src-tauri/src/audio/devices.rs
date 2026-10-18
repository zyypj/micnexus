//! Audio endpoints: which render device receives the phone's audio (normally
//! the input side of a virtual cable), its capture twin (what other apps pick
//! as "microphone"), and switching the Windows default microphone.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CableKind {
    Vbcable,
    Vac,
    Voicemeeter,
    Steam,
    Other,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub id: String,
    pub name: String,
    pub cable: Option<CableKind>,
    pub capture_name: Option<String>,
    #[serde(skip)]
    pub capture_id: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone)]
pub struct RawEndpoint {
    pub id: String,
    pub name: String,
    /// Name of the audio adapter ("VB-Audio Virtual Cable"); render and capture
    /// sides of a virtual cable share it.
    pub interface: String,
}

/// Which virtual cable (if any) an endpoint belongs to, from its names.
pub fn classify(name: &str, interface: &str) -> Option<CableKind> {
    let both = format!("{name} | {interface}").to_lowercase();
    if both.contains("vb-audio virtual cable") || both.contains("cable input") || both.contains("cable in 16") {
        Some(CableKind::Vbcable)
    } else if both.contains("voicemeeter") {
        Some(CableKind::Voicemeeter)
    } else if both.contains("steam streaming microphone") {
        Some(CableKind::Steam)
    } else if both.contains("virtual audio cable") {
        Some(CableKind::Vac)
    } else if both.contains("virtual") && both.contains("cable") {
        Some(CableKind::Other)
    } else {
        None
    }
}

/// Lower is better when picking automatically.
fn rank(e: &Endpoint) -> u32 {
    let base = match e.cable {
        Some(CableKind::Vbcable) => 0,
        Some(CableKind::Vac) => 10,
        Some(CableKind::Voicemeeter) => 20,
        Some(CableKind::Steam) => 30,
        Some(CableKind::Other) => 40,
        None => 1000,
    };
    // VB-CABLE also exposes "CABLE In 16ch": plain "CABLE Input" is the one.
    base + u32::from(e.name.contains("16ch")) + u32::from(e.capture_id.is_none()) * 5
}

/// Render endpoints with their cable classification and capture twin.
pub fn pair(render: Vec<RawEndpoint>, capture: &[RawEndpoint], default_render: Option<&str>) -> Vec<Endpoint> {
    let mut out: Vec<Endpoint> = render
        .into_iter()
        .map(|r| {
            let cable = classify(&r.name, &r.interface);
            let twin = cable.and_then(|_| {
                capture
                    .iter()
                    .find(|c| !r.interface.is_empty() && c.interface == r.interface)
                    .or_else(|| {
                        // VB-CABLE names: "CABLE Input" ↔ "CABLE Output".
                        let guess = r.name.replace("Input", "Output").replace("In 16ch", "Output");
                        capture.iter().find(|c| c.name == guess)
                    })
            });
            Endpoint {
                is_default: default_render == Some(r.id.as_str()),
                capture_name: twin.map(|c| c.name.clone()),
                capture_id: twin.map(|c| c.id.clone()),
                id: r.id,
                name: r.name,
                cable,
            }
        })
        .collect();
    out.sort_by_key(|e| (rank(e), e.name.clone()));
    out
}

/// The best virtual cable, if there is one.
pub fn best_cable(endpoints: &[Endpoint]) -> Option<&Endpoint> {
    endpoints.iter().find(|e| e.cable.is_some())
}

#[cfg(windows)]
pub use win::*;

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::c_void;
    use windows::{
        Win32::{
            Devices::FunctionDiscovery::{PKEY_Device_FriendlyName, PKEY_DeviceInterface_FriendlyName},
            Foundation::PROPERTYKEY,
            Media::Audio::{
                DEVICE_STATE_ACTIVE, EDataFlow, ERole, IAudioClient, IMMDevice, IMMDeviceEnumerator,
                MMDeviceEnumerator, eCapture, eCommunications, eConsole, eMultimedia, eRender,
            },
            System::Com::{
                CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                STGM_READ,
                StructuredStorage::{PropVariantClear, PropVariantToBSTR},
            },
        },
        core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, PCWSTR, interface},
    };

    /// Runs `f` with COM initialized on this thread (MTA).
    pub fn with_com<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let result = f();
        if initialized {
            unsafe { CoUninitialize() };
        }
        result
    }

    pub fn enumerator() -> Result<IMMDeviceEnumerator, String> {
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|e| format!("MMDeviceEnumerator: {e}"))
    }

    fn prop_string(device: &IMMDevice, key: &PROPERTYKEY) -> String {
        unsafe {
            let Ok(store) = device.OpenPropertyStore(STGM_READ) else {
                return String::new();
            };
            let Ok(mut value) = store.GetValue(key) else {
                return String::new();
            };
            let text = PropVariantToBSTR(&value).map(|b| b.to_string()).unwrap_or_default();
            let _ = PropVariantClear(&mut value);
            text
        }
    }

    fn device_id(device: &IMMDevice) -> Option<String> {
        unsafe {
            let id = device.GetId().ok()?;
            let text = id.to_string().ok();
            CoTaskMemFree(Some(id.0 as *const c_void));
            text
        }
    }

    fn list(enumerator: &IMMDeviceEnumerator, flow: EDataFlow) -> Result<Vec<RawEndpoint>, String> {
        let collection =
            unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) }.map_err(|e| e.to_string())?;
        let count = unsafe { collection.GetCount() }.map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for i in 0..count {
            let Ok(device) = (unsafe { collection.Item(i) }) else {
                continue;
            };
            let Some(id) = device_id(&device) else { continue };
            out.push(RawEndpoint {
                id,
                name: prop_string(&device, &PKEY_Device_FriendlyName),
                interface: prop_string(&device, &PKEY_DeviceInterface_FriendlyName),
            });
        }
        Ok(out)
    }

    pub fn default_id(enumerator: &IMMDeviceEnumerator, flow: EDataFlow, role: ERole) -> Option<String> {
        let device = unsafe { enumerator.GetDefaultAudioEndpoint(flow, role) }.ok()?;
        device_id(&device)
    }

    /// Render endpoints (cables first) for the output picker.
    pub fn outputs() -> Result<Vec<Endpoint>, String> {
        with_com(|| {
            let e = enumerator()?;
            let render = list(&e, eRender)?;
            let capture = list(&e, eCapture)?;
            let default = default_id(&e, eRender, eConsole);
            Ok(pair(render, &capture, default.as_deref()))
        })
    }

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct DefaultMics {
        pub console: Option<String>,
        pub communications: Option<String>,
    }

    pub fn default_mics() -> Result<DefaultMics, String> {
        with_com(|| {
            let e = enumerator()?;
            Ok(DefaultMics {
                console: default_id(&e, eCapture, eConsole),
                communications: default_id(&e, eCapture, eCommunications),
            })
        })
    }

    /// (sample rate, channels) the endpoint runs at in shared mode.
    pub fn mix_format(id: &str) -> Option<(u32, u16)> {
        with_com(|| {
            let e = enumerator()?;
            let device = unsafe { e.GetDevice(&windows::core::HSTRING::from(id)) }.map_err(|e| e.to_string())?;
            let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(|e| e.to_string())?;
            let format = unsafe { client.GetMixFormat() }.map_err(|e| e.to_string())?;
            let out = unsafe { ((*format).nSamplesPerSec, (*format).nChannels) };
            unsafe { CoTaskMemFree(Some(format as *const c_void)) };
            Ok(out)
        })
        .ok()
    }

    pub fn capture_exists(id: &str) -> bool {
        with_com(|| {
            let e = enumerator()?;
            Ok(list(&e, eCapture)?.iter().any(|c| c.id == id))
        })
        .unwrap_or(false)
    }

    // IPolicyConfig (undocumented, stable since Windows 7; used by the Sound
    // control panel itself). Only SetDefaultEndpoint is called; the other
    // slots just keep the vtable layout.
    #[allow(non_snake_case)]
    mod policy {
        use super::*;
        #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
        pub unsafe trait IPolicyConfig: IUnknown {
            fn GetMixFormat(&self, id: PCWSTR, format: *mut *mut c_void) -> HRESULT;
            fn GetDeviceFormat(&self, id: PCWSTR, default: i32, format: *mut *mut c_void) -> HRESULT;
            fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
            fn SetDeviceFormat(&self, id: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
            fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, def: *mut i64, min: *mut i64) -> HRESULT;
            fn SetProcessingPeriod(&self, id: PCWSTR, period: *mut i64) -> HRESULT;
            fn GetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
            fn SetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
            fn GetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
            fn SetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
            fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
            fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
        }

        const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

        /// Copies a WAVEFORMATEX(TENSIBLE) allocated by COM into owned bytes and frees it.
        unsafe fn take_format(ptr: *mut c_void) -> Option<Vec<u8>> {
            if ptr.is_null() {
                return None;
            }
            let cb = unsafe { (*(ptr as *const windows::Win32::Media::Audio::WAVEFORMATEX)).cbSize } as usize;
            let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, 18 + cb) }.to_vec();
            unsafe { CoTaskMemFree(Some(ptr as *const c_void)) };
            Some(bytes)
        }

        /// WAVEFORMATEX: nSamplesPerSec at byte 4, nAvgBytesPerSec at 8, nBlockAlign at 12.
        fn with_rate(mut f: Vec<u8>, rate: u32) -> Vec<u8> {
            let block = u16::from_le_bytes([f[12], f[13]]) as u32;
            f[4..8].copy_from_slice(&rate.to_le_bytes());
            f[8..12].copy_from_slice(&(rate * block).to_le_bytes());
            f
        }

        /// Changes an endpoint's shared-mode format to `rate` (what the
        /// Sound control panel's "Default Format" does; no elevation needed).
        pub fn set_rate(id: &str, rate: u32) -> Result<(), String> {
            with_com(|| {
                let policy: IPolicyConfig = unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) }
                    .map_err(|e| format!("PolicyConfig: {e}"))?;
                let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
                let id = PCWSTR(wide.as_ptr());
                let (mut device, mut mix) = (std::ptr::null_mut(), std::ptr::null_mut());
                unsafe {
                    policy
                        .GetDeviceFormat(id, 0, &mut device)
                        .ok()
                        .map_err(|e| format!("GetDeviceFormat: {e}"))?;
                    let device = take_format(device).ok_or("no device format")?;
                    policy
                        .GetMixFormat(id, &mut mix)
                        .ok()
                        .map_err(|e| format!("GetMixFormat: {e}"))?;
                    let mix = take_format(mix).ok_or("no mix format")?;
                    let mut device = with_rate(device, rate);
                    let mut mix = with_rate(mix, rate);
                    policy
                        .SetDeviceFormat(id, device.as_mut_ptr() as *mut c_void, mix.as_mut_ptr() as *mut c_void)
                        .ok()
                        .map_err(|e| format!("SetDeviceFormat: {e}"))
                }
            })
        }

        pub fn set_default(id: &str, roles: &[ERole]) -> Result<(), String> {
            with_com(|| {
                let policy: IPolicyConfig = unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) }
                    .map_err(|e| format!("PolicyConfig: {e}"))?;
                let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
                for role in roles {
                    unsafe { policy.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), *role) }
                        .ok()
                        .map_err(|e| format!("SetDefaultEndpoint: {e}"))?;
                }
                Ok(())
            })
        }
    }
    use policy::set_default;
    pub use policy::set_rate;

    /// Virtual cables copy raw samples from their render side to their
    /// capture side: if the two run at different rates the voice comes out
    /// pitched and glitchy. Puts both sides at 48 kHz (or, failing that, the
    /// capture side at the render side's rate).
    pub fn align_cable_rates(render_id: &str, capture_id: &str) -> Result<bool, String> {
        let (Some((r, _)), Some((c, _))) = (mix_format(render_id), mix_format(capture_id)) else {
            return Err("could not read the cable formats".into());
        };
        if r == c {
            return Ok(false);
        }
        let to_48k = (r == 48_000 || policy::set_rate(render_id, 48_000).is_ok())
            && (c == 48_000 || policy::set_rate(capture_id, 48_000).is_ok());
        if !to_48k {
            let r = mix_format(render_id).map(|f| f.0).unwrap_or(r);
            policy::set_rate(capture_id, r)?;
        }
        Ok(true)
    }

    /// Makes `id` the default microphone for every role.
    pub fn set_default_mic(id: &str) -> Result<(), String> {
        set_default(id, &[eConsole, eMultimedia, eCommunications])
    }

    /// Puts back what `default_mics` returned earlier (skipping devices that are gone).
    pub fn restore_default_mics(previous: &DefaultMics) -> Result<(), String> {
        if let Some(id) = previous.console.as_deref().filter(|id| capture_exists(id)) {
            set_default(id, &[eConsole, eMultimedia])?;
        }
        if let Some(id) = previous.communications.as_deref().filter(|id| capture_exists(id)) {
            set_default(id, &[eCommunications])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(id: &str, name: &str, interface: &str) -> RawEndpoint {
        RawEndpoint {
            id: id.into(),
            name: name.into(),
            interface: interface.into(),
        }
    }

    #[test]
    fn pairs_cables_with_their_capture_side() {
        let render = vec![
            raw("r1", "Speakers (Realtek(R) Audio)", "Realtek(R) Audio"),
            raw(
                "r2",
                "Alto-falantes (Steam Streaming Microphone)",
                "Steam Streaming Microphone",
            ),
            raw(
                "r3",
                "Alto-falantes (Steam Streaming Speakers)",
                "Steam Streaming Speakers",
            ),
            raw("r4", "CABLE In 16ch (VB-Audio Virtual Cable)", "VB-Audio Virtual Cable"),
            raw("r5", "CABLE Input (VB-Audio Virtual Cable)", "VB-Audio Virtual Cable"),
        ];
        let capture = vec![
            raw("c1", "Microphone Array (Realtek(R) Audio)", "Realtek(R) Audio"),
            raw(
                "c2",
                "Microfone (Steam Streaming Microphone)",
                "Steam Streaming Microphone",
            ),
            raw("c3", "CABLE Output (VB-Audio Virtual Cable)", "VB-Audio Virtual Cable"),
        ];
        let eps = pair(render, &capture, Some("r1"));
        let best = best_cable(&eps).unwrap();
        assert_eq!(best.id, "r5");
        assert_eq!(best.capture_id.as_deref(), Some("c3"));
        let steam = eps.iter().find(|e| e.id == "r2").unwrap();
        assert_eq!(steam.cable, Some(CableKind::Steam));
        assert_eq!(steam.capture_id.as_deref(), Some("c2"));
        let speakers = eps.iter().find(|e| e.id == "r3").unwrap();
        assert_eq!(speakers.cable, None, "Steam Streaming Speakers is not a microphone");
        assert!(eps.iter().find(|e| e.id == "r1").unwrap().is_default);
    }
}

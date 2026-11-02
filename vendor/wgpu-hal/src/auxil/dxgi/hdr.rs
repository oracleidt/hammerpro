
use parking_lot::Mutex;
use windows::{
    core::Interface as _,
    Win32::{
        Foundation::HWND,
        Graphics::{Dxgi, Gdi},
    },
};

type Primaries = [[f32; 2]; 3];

const REC709: Primaries = [[0.640, 0.330], [0.300, 0.600], [0.150, 0.060]];
const DISPLAY_P3: Primaries = [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060]];
const REC2020: Primaries = [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046]];

fn classify_gamut(red: [f32; 2], green: [f32; 2], blue: [f32; 2]) -> Option<wgt::DisplayGamut> {
    let measured = [red, green, blue];
    if measured.iter().all(|p| p[0] == 0.0 && p[1] == 0.0) {
        return None;
    }
    let dist = |reference: &Primaries| -> f32 {
        measured
            .iter()
            .zip(reference)
            .map(|(m, r)| {
                let dx = m[0] - r[0];
                let dy = m[1] - r[1];
                dx * dx + dy * dy
            })
            .sum()
    };
    [
        (wgt::DisplayGamut::Srgb, dist(&REC709)),
        (wgt::DisplayGamut::DisplayP3, dist(&DISPLAY_P3)),
        (wgt::DisplayGamut::Rec2020, dist(&REC2020)),
    ]
    .into_iter()
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .map(|(gamut, _)| gamut)
}

fn display_hdr_info_from_desc1(
    desc1: &Dxgi::DXGI_OUTPUT_DESC1,
    sdr_white_nits: Option<f32>,
) -> wgt::DisplayHdrInfo {
    let high_dynamic_range = desc1.ColorSpace
        == Dxgi::Common::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020
        || desc1.ColorSpace == Dxgi::Common::DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709;

    let luminance = wgt::DisplayLuminance {
        max_nits: Some(desc1.MaxLuminance),
        max_full_frame_nits: Some(desc1.MaxFullFrameLuminance),
        min_nits: Some(desc1.MinLuminance),
        sdr_white_nits,
    };

    let chromaticity = wgt::DisplayChromaticity {
        red: Some(desc1.RedPrimary),
        green: Some(desc1.GreenPrimary),
        blue: Some(desc1.BluePrimary),
        white: Some(desc1.WhitePoint),
    };

    let coarse = wgt::DisplayCoarseRange {
        high_dynamic_range: Some(high_dynamic_range),
        gamut: classify_gamut(desc1.RedPrimary, desc1.GreenPrimary, desc1.BluePrimary),
    };

    wgt::DisplayHdrInfo {
        luminance: Some(luminance),
        headroom: None,
        chromaticity: Some(chromaticity),
        coarse: Some(coarse),
        bits_per_color: u8::try_from(desc1.BitsPerColor).ok(),
    }
}

pub struct DxgiHdrSource {
    hwnd: HWND,
    factory: Mutex<Option<Dxgi::IDXGIFactory1>>,
}

unsafe impl Send for DxgiHdrSource {}
unsafe impl Sync for DxgiHdrSource {}

impl DxgiHdrSource {
    pub fn new(hwnd: HWND) -> Self {
        Self {
            hwnd,
            factory: Mutex::new(None),
        }
    }

    pub fn display_hdr_info(&self) -> Option<wgt::DisplayHdrInfo> {
        let desc1 = self.output_desc1()?;
        let sdr_white_nits = sdr_white_nits_for_monitor(desc1.Monitor);
        Some(display_hdr_info_from_desc1(&desc1, sdr_white_nits))
    }

    fn output_desc1(&self) -> Option<Dxgi::DXGI_OUTPUT_DESC1> {
        let mut factory = self.factory.lock();

        let need_new = factory
            .as_ref()
            .is_none_or(|f| !unsafe { f.IsCurrent() }.as_bool());
        if need_new {
            match unsafe { Dxgi::CreateDXGIFactory1() } {
                Ok(new) => *factory = Some(new),
                Err(e) => {
                    log::warn!("CreateDXGIFactory1 failed: {e}");
                    return None;
                }
            }
        }
        output_desc1_from_factory(factory.as_ref()?, self.hwnd)
    }
}

fn output_desc1_from_factory(
    factory: &Dxgi::IDXGIFactory1,
    wnd_handle: HWND,
) -> Option<Dxgi::DXGI_OUTPUT_DESC1> {
    let hmonitor = unsafe { Gdi::MonitorFromWindow(wnd_handle, Gdi::MONITOR_DEFAULTTONEAREST) };
    if hmonitor.is_invalid() {
        log::warn!("MonitorFromWindow failed; cannot identify the window's output");
        return None;
    }
    for adapter_index in 0.. {
        let adapter = match unsafe { factory.EnumAdapters1(adapter_index) } {
            Ok(adapter) => adapter,
            Err(e) if e.code() == Dxgi::DXGI_ERROR_NOT_FOUND => break,
            Err(e) => {
                log::warn!("IDXGIFactory1::EnumAdapters1 failed: {e}");
                break;
            }
        };
        for output_index in 0.. {
            let output = match unsafe { adapter.EnumOutputs(output_index) } {
                Ok(output) => output,
                Err(e) if e.code() == Dxgi::DXGI_ERROR_NOT_FOUND => break,
                Err(e) => {
                    log::warn!("IDXGIAdapter1::EnumOutputs failed: {e}");
                    break;
                }
            };
            let desc = match unsafe { output.GetDesc() } {
                Ok(desc) => desc,
                Err(e) => {
                    log::warn!("IDXGIOutput::GetDesc failed: {e}");
                    continue;
                }
            };
            if desc.Monitor != hmonitor {
                continue;
            }
            let output6 = match output.cast::<Dxgi::IDXGIOutput6>() {
                Ok(output6) => output6,
                Err(e) => {
                    log::warn!("Casting to IDXGIOutput6 failed: {e}");
                    return None;
                }
            };
            return match unsafe { output6.GetDesc1() } {
                Ok(desc1) => Some(desc1),
                Err(e) => {
                    log::warn!("IDXGIOutput6::GetDesc1 failed: {e}");
                    None
                }
            };
        }
    }
    log::warn!("No DXGI output matches the window's monitor");
    None
}

fn sdr_white_nits_for_monitor(hmonitor: Gdi::HMONITOR) -> Option<f32> {
    use windows::Win32::{
        Devices::Display::{
            DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
            DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_MODE_INFO,
            DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SDR_WHITE_LEVEL,
            DISPLAYCONFIG_SOURCE_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
        },
        Foundation::ERROR_SUCCESS,
        Graphics::Gdi::{GetMonitorInfoW, MONITORINFO, MONITORINFOEXW},
    };

    let mut monitor_info = MONITORINFOEXW::default();
    monitor_info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    let lpmi = core::ptr::from_mut(&mut monitor_info).cast::<MONITORINFO>();
    let got_info = unsafe { GetMonitorInfoW(hmonitor, lpmi) };
    if !got_info.as_bool() {
        return None;
    }
    let gdi_device_name = monitor_info.szDevice;

    let mut path_count = 0u32;
    let mut mode_count = 0u32;
    if unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    } != ERROR_SUCCESS
    {
        return None;
    }
    let mut paths = alloc::vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = alloc::vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
    if unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    } != ERROR_SUCCESS
    {
        return None;
    }

    for path in paths.iter().take(path_count as usize) {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        source.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        source.header.adapterId = path.sourceInfo.adapterId;
        source.header.id = path.sourceInfo.id;
        if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != ERROR_SUCCESS.0 as i32 {
            continue;
        }
        if source.viewGdiDeviceName != gdi_device_name {
            continue;
        }

        let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL::default();
        white.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL;
        white.header.size = size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32;
        white.header.adapterId = path.targetInfo.adapterId;
        white.header.id = path.targetInfo.id;
        if unsafe { DisplayConfigGetDeviceInfo(&mut white.header) } != ERROR_SUCCESS.0 as i32 {
            return None;
        }
        return (white.SDRWhiteLevel > 0).then(|| white.SDRWhiteLevel as f32 * 80.0 / 1000.0);
    }
    None
}

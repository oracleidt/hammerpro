use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use crate::settings::WindowPlacement;

pub const MIN_WIDTH: i32 = 320;
pub const MIN_HEIGHT: i32 = 240;
const TITLE_STRIP: i32 = 32;
const TITLE_VISIBLE: i32 = 120;

static MAIN_WINDOW: AtomicIsize = AtomicIsize::new(0);

pub fn set_main_window(handle: isize) {
    MAIN_WINDOW.store(handle, Ordering::Relaxed);
}

pub fn prepare_window(handle: isize) {
    #[cfg(windows)]
    {
        if win::is_window(handle) {
            win::clear_background(handle);
            win::remove_frame_strip(handle);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = handle;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PxRect {
    pub(crate) fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    fn right(self) -> i32 {
        self.x + self.w
    }

    fn bottom(self) -> i32 {
        self.y + self.h
    }

    fn overlap(self, other: PxRect) -> i64 {
        let w = (self.right().min(other.right()) - self.x.max(other.x)).max(0);
        let h = (self.bottom().min(other.bottom()) - self.y.max(other.y)).max(0);
        i64::from(w) * i64::from(h)
    }

    fn center_distance(self, other: PxRect) -> i64 {
        let dx = i64::from(self.x * 2 + self.w - other.x * 2 - other.w);
        let dy = i64::from(self.y * 2 + self.h - other.y * 2 - other.h);
        dx * dx + dy * dy
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Monitor {
    pub name: String,
    pub work: PxRect,
}

pub(crate) fn placement_of(rect: PxRect, monitor: Option<&Monitor>) -> WindowPlacement {
    WindowPlacement {
        x: rect.x,
        y: rect.y,
        width: rect.w,
        height: rect.h,
        monitor: monitor.map(|m| m.name.clone()).unwrap_or_default(),
        monitor_x: monitor.map_or(0, |m| m.work.x),
        monitor_y: monitor.map_or(0, |m| m.work.y),
    }
}

pub const USABLE_WIDTH: i32 = 480;
pub const USABLE_HEIGHT: i32 = 360;

pub fn is_usable_placement(p: &WindowPlacement) -> bool {
    p.width >= USABLE_WIDTH && p.height >= USABLE_HEIGHT
}

pub(crate) fn restore_rect(saved: &WindowPlacement, monitors: &[Monitor]) -> PxRect {
    let mut r = PxRect::new(saved.x, saved.y, saved.width.max(MIN_WIDTH), saved.height.max(MIN_HEIGHT));
    let monitors: Vec<&Monitor> = monitors.iter().filter(|m| m.work.w > 0 && m.work.h > 0).collect();
    if monitors.is_empty() {
        return r;
    }
    let named =
        (!saved.monitor.is_empty()).then(|| monitors.iter().find(|m| m.name == saved.monitor)).flatten().copied();
    if let Some(m) = named {
        r.x = r.x.saturating_add(m.work.x - saved.monitor_x);
        r.y = r.y.saturating_add(m.work.y - saved.monitor_y);
    }
    let title = PxRect::new(r.x, r.y, r.w, TITLE_STRIP);
    let title_shown = monitors.iter().any(|m| {
        let w = (title.right().min(m.work.right()) - title.x.max(m.work.x)).max(0);
        title.overlap(m.work) >= i64::from(w) * i64::from(TITLE_STRIP)
            && w >= TITLE_VISIBLE.min(r.w)
            && r.y >= m.work.y
            && r.y + TITLE_STRIP <= m.work.bottom()
    });
    if title_shown {
        return r;
    }
    let target = named.unwrap_or_else(|| {
        let best = monitors.iter().copied().max_by_key(|m| m.work.overlap(r)).filter(|m| m.work.overlap(r) > 0);
        best.unwrap_or_else(|| {
            monitors.iter().copied().min_by_key(|m| m.work.center_distance(r)).unwrap_or(monitors[0])
        })
    });
    clamp_into(r, target.work)
}

fn clamp_into(mut r: PxRect, area: PxRect) -> PxRect {
    r.w = r.w.min(area.w).max(1);
    r.h = r.h.min(area.h).max(1);
    r.x = r.x.clamp(area.x, area.right() - r.w);
    r.y = r.y.clamp(area.y, area.bottom() - r.h);
    r
}

#[derive(Debug, Default)]
pub struct NativeWindow {
    handle: isize,
    placed: bool,
    capture: Option<&'static str>,
}

static CAPTURE_WINDOWS: Mutex<Vec<(&'static str, isize, egui::ViewportId)>> = Mutex::new(Vec::new());

fn captures() -> std::sync::MutexGuard<'static, Vec<(&'static str, isize, egui::ViewportId)>> {
    CAPTURE_WINDOWS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn capture_handle(name: &str) -> isize {
    captures().iter().find(|(n, _, _)| *n == name).map_or(0, |(_, h, _)| *h)
}

pub fn viewport_of(name: &str) -> Option<egui::ViewportId> {
    captures().iter().find(|(n, _, _)| *n == name).map(|(_, _, v)| *v)
}

impl NativeWindow {
    pub fn follow(
        &mut self,
        ctx: &egui::Context,
        title: &str,
        restore: Option<&WindowPlacement>,
        capture: Option<&'static str>,
    ) -> Option<WindowPlacement> {
        if let Some(name) = capture {
            self.capture = Some(name);
        }
        #[cfg(windows)]
        {
            if self.handle == 0 || !win::is_window(self.handle) {
                self.handle = win::find_thread_window(title)?;
                self.placed = false;
                win::set_owner(self.handle, MAIN_WINDOW.load(Ordering::Relaxed));
                win::clear_background(self.handle);
                win::remove_frame_strip(self.handle);
            }
            if let Some(name) = capture {
                register_capture(name, self.handle, ctx.viewport_id());
            }
            if !self.placed {
                self.placed = true;
                if let Some(saved) = restore {
                    win::place(self.handle, restore_rect(saved, &win::monitors()));
                }
            }
            let (rect, monitor) = win::placement(self.handle)?;
            Some(placement_of(rect, monitor.as_ref()))
        }
        #[cfg(not(windows))]
        {
            let _ = (title, restore);
            if let Some(name) = capture {
                register_capture(name, 1, ctx.viewport_id());
            }
            self.placed = true;
            let (outer, inner, ppp) = ctx.input(|i| {
                let v = i.viewport();
                (v.outer_rect, v.inner_rect, v.native_pixels_per_point)
            });
            let (outer, inner, ppp) = (outer?, inner?, ppp? * ctx.zoom_factor());
            let r = PxRect::new(
                (outer.min.x * ppp).round() as i32,
                (outer.min.y * ppp).round() as i32,
                (inner.width() * ppp).round() as i32,
                (inner.height() * ppp).round() as i32,
            );
            Some(placement_of(r, None))
        }
    }

    pub fn forget(&mut self) {
        if let Some(name) = self.capture {
            captures().retain(|(n, _, _)| *n != name);
        }
        *self = Self::default();
    }
}

fn register_capture(name: &'static str, handle: isize, viewport: egui::ViewportId) {
    let mut list = captures();
    match list.iter_mut().find(|(n, _, _)| *n == name) {
        Some(entry) => (entry.1, entry.2) = (handle, viewport),
        None => list.push((name, handle, viewport)),
    }
}

pub fn builder(
    ctx: &egui::Context,
    title: &str,
    size: egui::Vec2,
    min_size: egui::Vec2,
    saved: Option<&WindowPlacement>,
    native: bool,
) -> egui::ViewportBuilder {
    let extra = if native || !crate::chrome::transparency_available() {
        egui::Vec2::ZERO
    } else {
        egui::Vec2::splat(2.0 * crate::chrome::MARGIN)
    };
    let (size, min_size) = (size + extra, min_size + extra);
    let mut b = egui::ViewportBuilder::default()
        .with_title(os_title(title))
        .with_inner_size(size)
        .with_min_inner_size(min_size)
        .with_resizable(true);
    b = crate::chrome::builder(b, native);
    let (main, ppp) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().native_pixels_per_point));
    let ppp = ppp.unwrap_or(1.0) * ctx.zoom_factor();
    match saved {
        Some(p) => {
            b = b.with_position(egui::pos2(p.x as f32 / ppp, p.y as f32 / ppp));
            b = b.with_inner_size(egui::vec2(p.width as f32 / ppp, p.height as f32 / ppp).max(min_size));
        }
        None => {
            if let Some(main) = main {
                b = b.with_position(main.center() - size * 0.5);
            }
        }
    }
    b
}

pub fn os_title(title: &str) -> String {
    format!("{title} - Hammer Pro")
}

pub fn capture_window(name: &str) -> Option<Result<egui::ColorImage, String>> {
    let handle = capture_handle(name);
    if handle == 0 {
        return None;
    }
    #[cfg(windows)]
    {
        if !win::is_window(handle) {
            return None;
        }
        Some(win::capture(handle))
    }
    #[cfg(not(windows))]
    {
        Some(Err("capturing a separate window is only supported on Windows".to_owned()))
    }
}

pub fn window_is_separate(name: &str) -> bool {
    let handle = capture_handle(name);
    #[cfg(windows)]
    {
        handle != 0 && win::is_window(handle)
    }
    #[cfg(not(windows))]
    {
        handle != 0
    }
}

pub fn separate_windows() -> Vec<&'static str> {
    let names: Vec<&'static str> = captures().iter().map(|(n, _, _)| *n).collect();
    names.into_iter().filter(|n| window_is_separate(n)).collect()
}

struct TextureUploader {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: std::sync::Arc<egui::mutex::RwLock<egui_wgpu::Renderer>>,
    frees: Vec<(u64, egui::TextureId)>,
}

static UPLOADER: Mutex<Option<TextureUploader>> = Mutex::new(None);

const FREE_DELAY: u64 = 2;

pub fn install_texture_uploader(render_state: &egui_wgpu::RenderState) {
    let mut slot = UPLOADER.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let frees = slot.take().map(|u| u.frees).unwrap_or_default();
    *slot = Some(TextureUploader {
        device: render_state.device.clone(),
        queue: render_state.queue.clone(),
        renderer: render_state.renderer.clone(),
        frees,
    });
}

pub fn upload_textures(ctx: &egui::Context) {
    let mut slot = UPLOADER.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(up) = slot.as_mut() else { return };
    let mut delta = ctx.tex_manager().write().take_delta();
    let (set, free) = (std::mem::take(&mut delta.set), std::mem::take(&mut delta.free));
    let frame = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT);
    let mut renderer = up.renderer.write();
    let mut later = Vec::new();
    for (id, images) in set {
        let partial = images.first().is_some_and(|i| i.pos.is_some()) && renderer.texture(&id).is_none();
        if partial || images.iter().any(|i| i.image.width() == 0 || i.image.height() == 0) {
            later.extend(images.into_iter().map(|i| (id, i)));
            continue;
        }
        for image in &images {
            renderer.update_texture(&up.device, &up.queue, id, image);
        }
    }
    up.frees.extend(free.into_iter().map(|id| (frame, id)));
    up.frees.retain(|(dropped, id)| {
        let keep = frame < dropped.saturating_add(FREE_DELAY);
        if !keep {
            renderer.free_texture(id);
        }
        keep
    });
    drop(renderer);
    drop(slot);
    if !later.is_empty() {
        let manager = ctx.tex_manager();
        let mut manager = manager.write();
        for (id, image) in later {
            manager.set(id, image);
        }
    }
}

#[cfg(windows)]
mod win {

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLACK_BRUSH, BitBlt, CAPTUREBLT, ClientToScreen, CreateCompatibleBitmap,
        CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, EnumDisplayMonitors, GetDC, GetDIBits,
        GetMonitorInfoW, GetStockObject, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW,
        MonitorFromWindow, RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE, RedrawWindow, ReleaseDC, SRCCOPY,
        SelectObject,
    };
    use windows_sys::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, GCLP_HBRBACKGROUND, GWLP_HWNDPARENT, GetClientRect, GetWindowRect, GetWindowTextW, IsIconic,
        IsWindow, IsZoomed, NCCALCSIZE_PARAMS, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SetClassLongPtrW, SetWindowLongPtrW, SetWindowPos, WM_NCCALCSIZE,
    };
    use windows_sys::core::BOOL;

    use super::{Monitor, PxRect};

    const PRINT_CLIENT_FULL: PRINT_WINDOW_FLAGS = 1 | 2;

    fn hwnd(handle: isize) -> HWND {
        handle as HWND
    }

    pub(super) fn is_window(handle: isize) -> bool {
        handle != 0 && unsafe { IsWindow(hwnd(handle)) } != 0
    }

    pub(super) fn find_thread_window(title: &str) -> Option<isize> {
        struct Search {
            title: Vec<u16>,
            found: isize,
        }
        unsafe extern "system" fn visit(window: HWND, param: LPARAM) -> BOOL {
            let search = unsafe { &mut *(param as *mut Search) };
            let mut buf = [0u16; 512];
            let n = unsafe { GetWindowTextW(window, buf.as_mut_ptr(), buf.len() as i32) };
            if n > 0 && buf[..n as usize] == search.title[..] {
                search.found = window as isize;
                return 0;
            }
            1
        }
        let mut search = Search { title: title.encode_utf16().collect(), found: 0 };
        unsafe { EnumThreadWindows(GetCurrentThreadId(), Some(visit), &mut search as *mut Search as LPARAM) };
        (search.found != 0).then_some(search.found)
    }

    pub(super) fn set_owner(window: isize, owner: isize) {
        if owner != 0 && owner != window && is_window(owner) {
            unsafe { SetWindowLongPtrW(hwnd(window), GWLP_HWNDPARENT, owner) };
        }
    }

    pub(super) fn clear_background(window: isize) {
        unsafe {
            let brush = GetStockObject(BLACK_BRUSH);
            SetClassLongPtrW(hwnd(window), GCLP_HBRBACKGROUND, brush as isize);
            RedrawWindow(
                hwnd(window),
                std::ptr::null(),
                std::ptr::null_mut(),
                RDW_ERASE | RDW_INVALIDATE | RDW_FRAME | RDW_ALLCHILDREN,
            );
        }
    }

    const FRAME_SUBCLASS: usize = 0x4850_4652;

    pub(super) fn remove_frame_strip(window: isize) {
        unsafe {
            SetWindowSubclass(hwnd(window), Some(frame_proc), FRAME_SUBCLASS, 0);
            SetWindowPos(
                hwnd(window),
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }

    unsafe extern "system" fn frame_proc(
        window: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        unsafe {
            if msg == WM_NCCALCSIZE && wparam != 0 && lparam != 0 && IsZoomed(window) == 0 {
                let params = lparam as *mut NCCALCSIZE_PARAMS;
                let before = (*params).rgrc[0];
                let result = DefSubclassProc(window, msg, wparam, lparam);
                let after = (*params).rgrc[0];
                let strip = after.left == before.left
                    && after.right == before.right
                    && after.top == before.top + 1
                    && after.bottom == before.bottom + 1;
                if strip {
                    (*params).rgrc[0] = before;
                }
                return result;
            }
            DefSubclassProc(window, msg, wparam, lparam)
        }
    }

    pub(super) fn place(window: isize, r: PxRect) {
        let flags = SWP_NOZORDER | SWP_NOACTIVATE;
        unsafe {
            SetWindowPos(hwnd(window), std::ptr::null_mut(), r.x, r.y, 0, 0, flags | SWP_NOSIZE);
            SetWindowPos(hwnd(window), std::ptr::null_mut(), 0, 0, r.w, r.h, flags | SWP_NOMOVE);
        }
    }

    fn px_rect(r: &RECT) -> PxRect {
        PxRect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
    }

    pub(super) fn placement(window: isize) -> Option<(PxRect, Option<Monitor>)> {
        let w = hwnd(window);
        unsafe {
            if IsIconic(w) != 0 || IsZoomed(w) != 0 {
                return None;
            }
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            if GetWindowRect(w, &mut r) == 0 {
                return None;
            }
            Some((px_rect(&r), monitor_info(MonitorFromWindow(w, MONITOR_DEFAULTTONEAREST))))
        }
    }

    fn monitor_info(monitor: HMONITOR) -> Option<Monitor> {
        let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) } == 0 {
            return None;
        }
        let name = &info.szDevice;
        let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        Some(Monitor { name: String::from_utf16_lossy(&name[..len]), work: px_rect(&info.monitorInfo.rcWork) })
    }

    pub(super) fn monitors() -> Vec<Monitor> {
        unsafe extern "system" fn visit(monitor: HMONITOR, _dc: HDC, _rect: *mut RECT, param: LPARAM) -> BOOL {
            let list = unsafe { &mut *(param as *mut Vec<Monitor>) };
            if let Some(m) = monitor_info(monitor) {
                list.push(m);
            }
            1
        }
        let mut list: Vec<Monitor> = Vec::new();
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(visit),
                &mut list as *mut Vec<Monitor> as LPARAM,
            )
        };
        list
    }

    pub(super) fn capture(window: isize) -> Result<egui::ColorImage, String> {
        let w = hwnd(window);
        unsafe {
            if IsIconic(w) != 0 {
                return Err("the window is minimized".to_owned());
            }
            let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            if GetClientRect(w, &mut client) == 0 || client.right <= 0 || client.bottom <= 0 {
                return Err("the window has no size".to_owned());
            }
            let (width, height) = (client.right, client.bottom);
            let screen = GetDC(std::ptr::null_mut());
            let mem = CreateCompatibleDC(screen);
            let bitmap = CreateCompatibleBitmap(screen, width, height);
            let old = SelectObject(mem, bitmap);
            let mut pixels = None;
            if PrintWindow(w, mem, PRINT_CLIENT_FULL) != 0 {
                pixels = read_bits(mem, bitmap, width, height).filter(|p| !all_black(p));
            }
            if pixels.is_none() {
                let mut origin = POINT { x: 0, y: 0 };
                ClientToScreen(w, &mut origin);
                if BitBlt(mem, 0, 0, width, height, screen, origin.x, origin.y, SRCCOPY | CAPTUREBLT) != 0 {
                    pixels = read_bits(mem, bitmap, width, height);
                }
            }
            SelectObject(mem, old);
            DeleteObject(bitmap);
            DeleteDC(mem);
            ReleaseDC(std::ptr::null_mut(), screen);
            let pixels = pixels.ok_or("cannot read the pixels of the window")?;
            Ok(egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &pixels))
        }
    }

    fn all_black(rgba: &[u8]) -> bool {
        rgba.chunks_exact(4).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0)
    }

    unsafe fn read_bits(
        dc: HDC,
        bitmap: windows_sys::Win32::Graphics::Gdi::HBITMAP,
        width: i32,
        height: i32,
    ) -> Option<Vec<u8>> {
        let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..info.bmiHeader
        };
        let mut bgra = vec![0u8; width as usize * height as usize * 4];
        let rows =
            unsafe { GetDIBits(dc, bitmap, 0, height as u32, bgra.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS) };
        if rows != height {
            return None;
        }
        for p in bgra.chunks_exact_mut(4) {
            p.swap(0, 2);
            p[3] = 255;
        }
        Some(bgra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(name: &str, x: i32, y: i32, w: i32, h: i32) -> Monitor {
        Monitor { name: name.to_owned(), work: PxRect::new(x, y, w, h) }
    }

    fn two_screens() -> Vec<Monitor> {
        vec![mon("\\\\.\\DISPLAY1", 0, 0, 2560, 1400), mon("\\\\.\\DISPLAY2", 2560, 0, 1920, 1040)]
    }

    fn saved(x: i32, y: i32, w: i32, h: i32, monitor: &str, mx: i32, my: i32) -> WindowPlacement {
        WindowPlacement { x, y, width: w, height: h, monitor: monitor.to_owned(), monitor_x: mx, monitor_y: my }
    }

    #[test]
    fn a_squashed_window_is_not_a_usable_place() {
        let m = two_screens();
        assert!(!is_usable_placement(&placement_of(PxRect::new(0, 0, 900, 64), Some(&m[0]))));
        assert!(!is_usable_placement(&placement_of(PxRect::new(0, 0, 200, 700), Some(&m[0]))));
        assert!(is_usable_placement(&placement_of(PxRect::new(0, 0, 900, 700), Some(&m[0]))));
    }

    #[test]
    fn same_screens_keep_the_place() {
        let s = saved(2700, 100, 900, 700, "\\\\.\\DISPLAY2", 2560, 0);
        assert_eq!(restore_rect(&s, &two_screens()), PxRect::new(2700, 100, 900, 700));
        let s = saved(2200, 200, 900, 700, "\\\\.\\DISPLAY1", 0, 0);
        assert_eq!(restore_rect(&s, &two_screens()), PxRect::new(2200, 200, 900, 700));
        let m = two_screens();
        let p = placement_of(PxRect::new(100, 50, 800, 600), Some(&m[0]));
        assert_eq!(restore_rect(&p, &m), PxRect::new(100, 50, 800, 600));
    }

    #[test]
    fn the_window_follows_its_screen() {
        let moved = vec![mon("\\\\.\\DISPLAY1", 0, 0, 2560, 1400), mon("\\\\.\\DISPLAY2", -1920, 0, 1920, 1040)];
        let s = saved(2700, 100, 900, 700, "\\\\.\\DISPLAY2", 2560, 0);
        assert_eq!(restore_rect(&s, &moved), PxRect::new(-1780, 100, 900, 700));
    }

    #[test]
    fn a_window_of_a_screen_gone_comes_back() {
        let one = vec![mon("\\\\.\\DISPLAY1", 0, 0, 2560, 1400)];
        let s = saved(2700, 100, 900, 700, "\\\\.\\DISPLAY2", 2560, 0);
        let r = restore_rect(&s, &one);
        assert_eq!(r, PxRect::new(2560 - 900, 100, 900, 700), "pushed inside the screen left");
        let s = saved(2600, -50, 3000, 2000, "\\\\.\\DISPLAY2", 2560, 0);
        assert_eq!(restore_rect(&s, &one), PxRect::new(0, 0, 2560, 1400));
        let s = saved(-9000, 5000, 800, 600, "", 0, 0);
        let r = restore_rect(&s, &two_screens());
        assert_eq!(r, PxRect::new(0, 1400 - 600, 800, 600));
    }

    #[test]
    fn the_title_bar_stays_reachable() {
        let s = saved(400, -300, 900, 700, "\\\\.\\DISPLAY1", 0, 0);
        assert_eq!(restore_rect(&s, &two_screens()), PxRect::new(400, 0, 900, 700));
        let s = saved(2540, 100, 900, 700, "", 0, 0);
        let r = restore_rect(&s, &[mon("\\\\.\\DISPLAY1", 0, 0, 2560, 1400)]);
        assert_eq!(r, PxRect::new(2560 - 900, 100, 900, 700));
        let s = saved(100, 1300, 900, 700, "\\\\.\\DISPLAY1", 0, 0);
        assert_eq!(restore_rect(&s, &two_screens()), PxRect::new(100, 1300, 900, 700));
    }

    #[test]
    fn damaged_sizes_and_no_screens() {
        let s = saved(10, 10, 0, -5, "", 0, 0);
        assert_eq!(restore_rect(&s, &two_screens()), PxRect::new(10, 10, MIN_WIDTH, MIN_HEIGHT));
        let s = saved(-50_000, 99_999, 700, 500, "x", 0, 0);
        assert_eq!(restore_rect(&s, &[]), PxRect::new(-50_000, 99_999, 700, 500), "nothing to check against");
        let empty = [mon("\\\\.\\DISPLAY9", 0, 0, 0, 0)];
        assert_eq!(
            restore_rect(&s, &empty),
            PxRect::new(-50_000, 99_999, 700, 500),
            "a screen without size is left out"
        );
        assert_eq!(os_title("Compile and run"), "Compile and run - Hammer Pro");
    }

    #[test]
    fn capture_registry() {
        let mut w = NativeWindow::default();
        assert!(!window_is_separate("test_window"));
        register_capture("test_window", 0, egui::ViewportId::ROOT);
        assert_eq!(capture_handle("test_window"), 0);
        assert_eq!(viewport_of("test_window"), Some(egui::ViewportId::ROOT));
        w.capture = Some("test_window");
        w.forget();
        assert!(captures().iter().all(|(n, _, _)| *n != "test_window"));
        assert_eq!(viewport_of("test_window"), None);
        assert!(capture_window("test_window").is_none());
    }

    #[test]
    fn builders_follow_the_frame_setting() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
        let b = builder(&ctx, "UV editor", egui::vec2(600.0, 800.0), egui::vec2(300.0, 300.0), None, true);
        assert_eq!(b.decorations, Some(true));
        assert_eq!(b.transparent, Some(false));
        assert_eq!(b.inner_size, Some(egui::vec2(600.0, 800.0)));
        let b = builder(&ctx, "UV editor", egui::vec2(600.0, 800.0), egui::vec2(300.0, 300.0), None, false);
        assert_eq!(b.decorations, Some(false));
        assert_eq!(b.title.as_deref(), Some("UV editor - Hammer Pro"));
    }
}

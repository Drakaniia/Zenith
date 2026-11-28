//! Native Win32 child-window placement for CEF browsers: rectangles, animation clipping via
//! container HWNDs, visibility, overlay-lease z-order, relayout on resize, and fullscreen.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, FillRect,
    SetWindowRgn, HDC, RGN_DIFF, RGN_OR,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{window_of_tab, with_host, with_win, Host, LayoutSpec, PaneSpec};

static CONTAINER_CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn container_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_ERASEBKGND {
        let (r, g, b) = super::HOST
            .with(|c| c.try_borrow().ok().map(|h| h.page_bg))
            .unwrap_or((32, 32, 36));
        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_ok() {
            let color = COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16));
            let brush = CreateSolidBrush(color);
            if !brush.is_invalid() {
                let _ = FillRect(HDC(wparam.0 as *mut _), &rect, brush);
                let _ = DeleteObject(brush.into());
            }
        }
        return LRESULT(1);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

pub(crate) fn create_container(parent: HWND, x: i32, y: i32, w: i32, h: i32) -> Result<HWND, String> {
    unsafe {
        let hinstance = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
        if !CONTAINER_CLASS_REGISTERED.swap(true, Ordering::SeqCst) {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(container_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: w!("ZenithTabContainer"),
                ..Default::default()
            };
            RegisterClassW(&wc);
        }
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("ZenithTabContainer"),
            w!(""),
            WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            x,
            y,
            w.max(1),
            h.max(1),
            Some(parent),
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| format!("CreateWindowExW tab container: {e}"))
    }
}

/// Converts a logical CSS rectangle into physical window pixels without edge wobble.
pub(crate) fn px_rect(x: f64, y: f64, w: f64, h: f64, scale: f64) -> (i32, i32, i32, i32) {
    let left = (x * scale).round() as i32;
    let top = (y * scale).round() as i32;
    let pw = (((x + w) * scale).round() as i32 - left).max(1);
    let ph = (((y + h) * scale).round() as i32 - top).max(1);
    (left, top, pw, ph)
}

fn content_px(p: &PaneSpec, scale: f64) -> (i32, i32) {
    let (_, _, w, h) = px_rect(p.x, p.y, p.w, p.h, scale);
    let cw = if p.cw > p.w { (p.cw * scale).round() as i32 } else { 0 };
    let ch = if p.ch > p.h { (p.ch * scale).round() as i32 } else { 0 };
    (cw.max(w), ch.max(h))
}

/// Logical corner radius of Zen's content cards (`--r-content: 6px`).
const CARD_RADIUS_CSS: f64 = 6.0;

fn defer_or_set_pos(
    hdwp: &mut Option<HDWP>,
    hwnd: HWND,
    after: Option<HWND>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    flags: SET_WINDOW_POS_FLAGS,
) {
    unsafe {
        if let Some(cur) = *hdwp {
            match DeferWindowPos(cur, hwnd, after, x, y, w, h, flags) {
                Ok(next) if !next.is_invalid() => {
                    *hdwp = Some(next);
                    return;
                }
                _ => {
                    // DeferWindowPos invalidated the batch: fall back to individual SetWindowPos.
                    *hdwp = None;
                }
            }
        }
        let _ = SetWindowPos(hwnd, after, x, y, w, h, flags);
    }
}

fn update_container_rgn(container: HWND, w: i32, h: i32, radius_px: i32, cached: &mut Option<(i32, i32, i32)>) {
    let key = (w, h, radius_px);
    if *cached == Some(key) || container.is_invalid() {
        return;
    }
    unsafe {
        if radius_px <= 0 {
            let _ = SetWindowRgn(container, None, false);
        } else {
            let d = radius_px * 2;
            // CreateRoundRectRgn excludes the right/bottom bound, so +1 covers [0..w, 0..h].
            let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, d, d);
            if !rgn.is_invalid() && SetWindowRgn(container, Some(rgn), false) == 0 {
                let _ = DeleteObject(rgn.into());
            }
        }
    }
    *cached = Some(key);
}

/// Places a tab's container HWND and sizes its inner CEF browser HWND. Keeping the inner
/// browser at `(cw, ch)` while the container clips to `(w, h)` avoids per-frame relayouts
/// during sidebar/compact/panel animations, and omitting `SWP_NOCOPYBITS` on container moves
/// preserves painted pixels so sliding never flashes blank.
#[allow(clippy::too_many_arguments)]
fn place_tab(
    hdwp: &mut Option<HDWP>,
    container: HWND,
    browser_hwnd: HWND,
    p: &PaneSpec,
    prev: Option<(f64, f64, f64, f64)>,
    content: &mut Option<(i32, i32)>,
    rgn_px: &mut Option<(i32, i32, i32)>,
    radius_px: i32,
    scale: f64,
) {
    let want = content_px(p, scale);
    let (left, top, w, h) = px_rect(p.x, p.y, p.w, p.h, scale);
    let same_size = prev.is_some_and(|(x, y, pw, ph)| {
        let (_, _, ow, oh) = px_rect(x, y, pw, ph, scale);
        (ow, oh) == (w, h)
    });
    if *content != Some(want) && !browser_hwnd.is_invalid() {
        super::perf::note_resize();
        // browser_hwnd is a child of `container` (different parent than `hdwp`), so size directly.
        unsafe {
            let _ = SetWindowPos(
                browser_hwnd,
                None,
                0,
                0,
                want.0,
                want.1,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
        *content = Some(want);
    }
    if !container.is_invalid() {
        update_container_rgn(container, w, h, radius_px, rgn_px);
        if same_size {
            defer_or_set_pos(
                hdwp,
                container,
                None,
                left,
                top,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
            );
        } else {
            defer_or_set_pos(
                hdwp,
                container,
                None,
                left,
                top,
                w,
                h,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
    }
}

impl Host {
    pub(super) fn scale(&self, label: &str) -> f64 {
        let Some(win) = self.wins.get(label) else { return 1.0 };
        let dpi = unsafe { GetDpiForWindow(win.hwnd) };
        if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 }
    }

    pub fn pane_for(&self, label: &str, id: &str) -> Option<PaneSpec> {
        let layout = &self.wins.get(label)?.layout;
        layout
            .panes
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .or_else(|| layout.glance.clone().filter(|g| g.id == id))
            .or_else(|| layout.ext_popup.clone().filter(|p| p.id == id))
    }

    pub fn apply_layout(&mut self, label: &str) {
        let scale = self.scale(label);
        let Some(win) = self.wins.get(label) else { return };
        let mut visible: HashMap<String, PaneSpec> = HashMap::new();
        if let Some(fs) = win.fullscreen_tab.clone().filter(|id| self.tabs.contains_key(id)) {
            visible.insert(
                fs.clone(),
                PaneSpec {
                    id: fs,
                    x: 0.0,
                    y: 0.0,
                    w: win.layout.viewport_w,
                    h: win.layout.viewport_h,
                    ..Default::default()
                },
            );
        } else {
            for p in &win.layout.panes {
                visible.insert(p.id.clone(), p.clone());
            }
            if let Some(g) = &win.layout.glance {
                visible.insert(g.id.clone(), g.clone());
            }
            if let Some(ep) = &win.layout.ext_popup {
                visible.insert(ep.id.clone(), ep.clone());
            }
        }

        let fullscreen = win.fullscreen_tab.is_some();
        let radius_px = if fullscreen { 0 } else { (CARD_RADIUS_CSS * scale).round().max(0.0) as i32 };
        let visible_dt = if !fullscreen {
            win.layout.devtools.clone()
        } else {
            None
        };

        let batch_cap = (visible.len() + usize::from(visible_dt.is_some()) + 2) as i32;
        let mut hdwp = unsafe { BeginDeferWindowPos(batch_cap).ok().filter(|h| !h.is_invalid()) };

        for (id, tab) in self.tabs.iter_mut() {
            if tab.window() != label {
                continue;
            }
            match visible.get(id) {
                Some(p) => {
                    let rect = (p.x, p.y, p.w, p.h);
                    let want = content_px(p, scale);
                    let (_, _, pw, ph) = px_rect(p.x, p.y, p.w, p.h, scale);
                    if tab.last_rect != Some(rect)
                        || tab.content_px != Some(want)
                        || tab.rgn_px != Some((pw, ph, radius_px))
                    {
                        place_tab(
                            &mut hdwp,
                            tab.container_hwnd,
                            tab.browser_hwnd,
                            p,
                            tab.last_rect,
                            &mut tab.content_px,
                            &mut tab.rgn_px,
                            radius_px,
                            scale,
                        );
                        tab.last_rect = Some(rect);
                    }
                    if !tab.visible {
                        tab.set_visible(true);
                    }
                }
                None => {
                    if tab.visible {
                        tab.set_visible(false);
                    }
                }
            }
            if let Some(dt) = tab.devtools.as_mut() {
                match visible_dt.as_ref().filter(|d| d.id == *id) {
                    Some(p) => {
                        let rect = (p.x, p.y, p.w, p.h);
                        let want = content_px(p, scale);
                        let (_, _, pw, ph) = px_rect(p.x, p.y, p.w, p.h, scale);
                        if dt.last_rect != Some(rect)
                            || dt.content_px != Some(want)
                            || dt.rgn_px != Some((pw, ph, radius_px))
                        {
                            place_tab(
                                &mut hdwp,
                                dt.container_hwnd,
                                dt.browser_hwnd,
                                p,
                                dt.last_rect,
                                &mut dt.content_px,
                                &mut dt.rgn_px,
                                radius_px,
                                scale,
                            );
                            dt.last_rect = Some(rect);
                        }
                        if !dt.visible {
                            dt.set_visible(true);
                        }
                    }
                    None => {
                        if dt.visible {
                            dt.set_visible(false);
                        }
                    }
                }
            }
        }
        if let Some(h) = hdwp.take() {
            unsafe {
                let _ = EndDeferWindowPos(h);
            }
        }
        self.apply_zorder(label);
    }

    pub fn apply_zorder(&mut self, label: &str) {
        let scale = self.scale(label);
        let Some(win) = self.wins.get_mut(label) else { return };
        let raise = win.layout.overlay && win.fullscreen_tab.is_none();
        let rgn_key = if raise && (!win.layout.panes.is_empty() || win.layout.devtools.is_some()) {
            let mut key = Vec::with_capacity(
                1 + win.layout.panes.len() + usize::from(win.layout.devtools.is_some()) + win.layout.overlay_rects.len(),
            );
            let (_, _, vw, vh) = px_rect(0.0, 0.0, win.layout.viewport_w, win.layout.viewport_h, scale);
            key.push((0, 0, vw, vh, 0));
            for p in &win.layout.panes {
                let (x, y, w, h) = px_rect(p.x, p.y, p.w, p.h, scale);
                key.push((x, y, x + w, y + h, -1));
            }
            if let Some(d) = &win.layout.devtools {
                let (x, y, w, h) = px_rect(d.x, d.y, d.w, d.h, scale);
                key.push((x, y, x + w, y + h, -1));
            }
            for o in &win.layout.overlay_rects {
                let (x, y, w, h) = px_rect(o.x, o.y, o.w, o.h, scale);
                let r = (o.r * scale).round().max(0.0) as i32;
                key.push((x, y, x + w, y + h, r));
            }
            Some(key)
        } else {
            None
        };
        unsafe {
            if !win.chrome_hwnd.is_invalid() {
                if win.chrome_rgn_key != rgn_key {
                    if let Some(ref key) = rgn_key {
                        let (l, t, r, b, _) = key[0];
                        let rgn = CreateRectRgn(l, t, r, b);
                        if !rgn.is_invalid() {
                            for &(pl, pt, pr, pb, kind) in &key[1..] {
                                let part = if kind < 0 {
                                    CreateRectRgn(pl, pt, pr, pb)
                                } else if kind > 0 {
                                    CreateRoundRectRgn(pl, pt, pr, pb, kind * 2, kind * 2)
                                } else {
                                    CreateRectRgn(pl, pt, pr, pb)
                                };
                                if !part.is_invalid() {
                                    let _ = CombineRgn(Some(rgn), Some(rgn), Some(part), if kind < 0 { RGN_DIFF } else { RGN_OR });
                                    let _ = DeleteObject(part.into());
                                }
                            }
                            if SetWindowRgn(win.chrome_hwnd, Some(rgn), true) == 0 {
                                let _ = DeleteObject(rgn.into());
                            }
                        }
                    } else {
                        let _ = SetWindowRgn(win.chrome_hwnd, None, true);
                    }
                    win.chrome_rgn_key = rgn_key;
                }
                if raise {
                    let _ = SetWindowPos(
                        win.chrome_hwnd,
                        Some(HWND_TOP),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                    if let Some(g) = &win.layout.glance {
                        if let Some(t) = self.tabs.get(&g.id) {
                            if !t.container_hwnd.is_invalid() {
                                let _ = SetWindowPos(
                                    t.container_hwnd,
                                    Some(HWND_TOP),
                                    0,
                                    0,
                                    0,
                                    0,
                                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                                );
                            }
                        }
                    }
                    if let Some(ep) = &win.layout.ext_popup {
                        if let Some(t) = self.tabs.get(&ep.id) {
                            if !t.container_hwnd.is_invalid() {
                                let _ = SetWindowPos(
                                    t.container_hwnd,
                                    Some(HWND_TOP),
                                    0,
                                    0,
                                    0,
                                    0,
                                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                                );
                            }
                        }
                    }
                } else if win.chrome_raised != Some(false) {
                    let _ = SetWindowPos(
                        win.chrome_hwnd,
                        Some(HWND_BOTTOM),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
        }
        win.chrome_raised = Some(raise);
    }

    fn relayout_for_size(&mut self, label: &str, width_px: i32, height_px: i32) {
        if let Some(win) = self.wins.get(label) {
            if !win.chrome_hwnd.is_invalid() {
                unsafe {
                    let _ = SetWindowPos(
                        win.chrome_hwnd,
                        None,
                        0,
                        0,
                        width_px.max(1),
                        height_px.max(1),
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                }
            }
        }
        let scale = self.scale(label);
        let (new_w, new_h) = (width_px as f64 / scale, height_px as f64 / scale);
        let Some(win) = self.wins.get_mut(label) else { return };
        let old = win.layout.clone();
        if old.viewport_w <= 0.0 || old.viewport_h <= 0.0 {
            return;
        }
        let map_group = |panes: &[PaneSpec]| -> Vec<PaneSpec> {
            if panes.is_empty() {
                return Vec::new();
            }
            let min_x = panes.iter().map(|p| p.x).fold(f64::MAX, f64::min);
            let min_y = panes.iter().map(|p| p.y).fold(f64::MAX, f64::min);
            let max_x = panes.iter().map(|p| p.x + p.w).fold(f64::MIN, f64::max);
            let max_y = panes.iter().map(|p| p.y + p.h).fold(f64::MIN, f64::max);
            let (right, bottom) = (old.viewport_w - max_x, old.viewport_h - max_y);
            let (cw, ch) = ((max_x - min_x).max(1.0), (max_y - min_y).max(1.0));
            let (ncw, nch) = ((new_w - right - min_x).max(1.0), (new_h - bottom - min_y).max(1.0));
            panes
                .iter()
                .map(|p| PaneSpec {
                    id: p.id.clone(),
                    x: min_x + (p.x - min_x) / cw * ncw,
                    y: min_y + (p.y - min_y) / ch * nch,
                    w: p.w / cw * ncw,
                    h: p.h / ch * nch,
                    ..Default::default()
                })
                .collect()
        };
        win.layout.panes = map_group(&old.panes);
        if let Some(g) = &old.glance {
            let fx = new_w / old.viewport_w;
            let fy = new_h / old.viewport_h;
            win.layout.glance = Some(PaneSpec {
                id: g.id.clone(),
                x: g.x * fx,
                y: g.y * fy,
                w: g.w * fx,
                h: g.h * fy,
                ..Default::default()
            });
        }
        if let Some(ep) = &old.ext_popup {
            let fx = new_w / old.viewport_w;
            let fy = new_h / old.viewport_h;
            win.layout.ext_popup = Some(PaneSpec {
                id: ep.id.clone(),
                x: ep.x * fx,
                y: ep.y * fy,
                w: ep.w * fx,
                h: ep.h * fy,
                ..Default::default()
            });
        }
        if let Some(d) = &old.devtools {
            let fx = new_w / old.viewport_w;
            let fy = new_h / old.viewport_h;
            win.layout.devtools = Some(PaneSpec {
                id: d.id.clone(),
                x: d.x * fx,
                y: d.y * fy,
                w: d.w * fx,
                h: d.h * fy,
                ..Default::default()
            });
        }
        win.layout.viewport_w = new_w;
        win.layout.viewport_h = new_h;
        self.apply_layout(label);
    }
}

pub(super) fn on_resize(hwnd: HWND, width_px: i32, height_px: i32) {
    with_host(|h| {
        let label = h.wins.iter().find(|(_, w)| w.hwnd == hwnd).map(|(l, _)| l.clone());
        if let Some(label) = label {
            h.relayout_for_size(&label, width_px, height_px);
        }
    });
}

pub fn set_layout(label: &str, spec: LayoutSpec) {
    with_host(|h| {
        if let Some(win) = h.wins.get_mut(label) {
            win.layout = spec;
            h.apply_layout(label);
        }
    });
}

pub fn set_fullscreen_tab(tab_id: &str, on: bool) {
    let label = window_of_tab(tab_id);
    let window = with_win(&label, |w| {
        w.fullscreen_tab = on.then(|| tab_id.to_string());
        w.window.clone()
    });
    if let Some(window) = window {
        let _ = window.set_fullscreen(on);
    }
    with_host(|h| h.apply_layout(&label));
}

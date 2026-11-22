//! Native Win32 child-window placement for CEF browsers: rectangles, animation clipping via
//! container HWNDs, visibility, overlay-lease z-order, relayout on resize, and fullscreen.

use std::collections::HashMap;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{window_of_tab, with_host, with_win, Host, LayoutSpec, PaneSpec};

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

/// Places a tab's container HWND and sizes its inner CEF browser HWND. Keeping the inner
/// browser at `(cw, ch)` while the container clips to `(w, h)` avoids per-frame relayouts
/// during sidebar/compact animations.
fn place_tab(
    container: HWND,
    browser_hwnd: HWND,
    p: &PaneSpec,
    prev: Option<(f64, f64, f64, f64)>,
    content: &mut Option<(i32, i32)>,
    scale: f64,
) {
    let want = content_px(p, scale);
    let (left, top, w, h) = px_rect(p.x, p.y, p.w, p.h, scale);
    let same_size = prev.is_some_and(|(x, y, pw, ph)| {
        let (_, _, ow, oh) = px_rect(x, y, pw, ph, scale);
        (ow, oh) == (w, h)
    });
    unsafe {
        if *content != Some(want) && !browser_hwnd.is_invalid() {
            let _ = SetWindowPos(browser_hwnd, None, 0, 0, want.0, want.1, SWP_NOACTIVATE | SWP_NOZORDER);
            *content = Some(want);
        }
        if !container.is_invalid() {
            if same_size {
                let _ = SetWindowPos(container, None, left, top, 0, 0, SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE);
            } else {
                let _ = SetWindowPos(container, None, left, top, w, h, SWP_NOACTIVATE | SWP_NOZORDER);
            }
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
        }

        for (id, tab) in self.tabs.iter_mut() {
            if tab.shared.window != label {
                continue;
            }
            match visible.get(id) {
                Some(p) => {
                    let rect = (p.x, p.y, p.w, p.h);
                    let want = content_px(p, scale);
                    if tab.last_rect != Some(rect) || tab.content_px != Some(want) {
                        place_tab(tab.container_hwnd, tab.browser_hwnd, p, tab.last_rect, &mut tab.content_px, scale);
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
        }
        self.apply_zorder(label);
    }

    pub fn apply_zorder(&mut self, label: &str) {
        let Some(win) = self.wins.get_mut(label) else { return };
        let raise = win.layout.overlay && win.fullscreen_tab.is_none();
        unsafe {
            if !win.chrome_hwnd.is_invalid() {
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

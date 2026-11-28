//! Read-only layout snapshots for the end-to-end test suite.

use serde::Serialize;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

use super::{with_host, Focus};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugTab {
    id: String,
    target_id: String,
    requests: u32,
    blocked: u32,
    visible: bool,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugLayout {
    chrome_raised: bool,
    focus: String,
    tabs: Vec<DebugTab>,
    zombies: usize,
    downloads: usize,
    permissions: usize,
}

pub fn debug_layout(label: &str) -> Option<DebugLayout> {
    with_host(|h| {
        let scale = h.scale(label);
        let win = h.wins.get(label)?;
        let parent_hwnd = win.hwnd;
        let mut tabs: Vec<DebugTab> = h
            .tabs
            .iter()
            .filter(|(_, t)| t.window() == label)
            .map(|(id, t)| {
                let (x, y, w, hh) = if !t.container_hwnd.is_invalid() {
                    let mut r = RECT::default();
                    unsafe {
                        if GetWindowRect(t.container_hwnd, &mut r).is_ok() {
                            let mut tl = POINT { x: r.left, y: r.top };
                            let mut br = POINT { x: r.right, y: r.bottom };
                            let _ = ScreenToClient(parent_hwnd, &mut tl);
                            let _ = ScreenToClient(parent_hwnd, &mut br);
                            (
                                tl.x as f64 / scale,
                                tl.y as f64 / scale,
                                (br.x - tl.x).max(0) as f64 / scale,
                                (br.y - tl.y).max(0) as f64 / scale,
                            )
                        } else {
                            t.last_rect.unwrap_or_default()
                        }
                    }
                } else {
                    t.last_rect.unwrap_or_default()
                };
                DebugTab {
                    id: id.clone(),
                    target_id: t.shared.target_id(),
                    requests: t.shared.requests(),
                    blocked: t.shared.blocked(),
                    visible: t.visible,
                    x,
                    y,
                    w,
                    h: hh,
                }
            })
            .collect();
        tabs.sort_by(|a, b| a.id.cmp(&b.id));
        Some(DebugLayout {
            chrome_raised: win.chrome_raised.unwrap_or(false),
            focus: match &win.focus {
                Focus::Chrome => "chrome".into(),
                Focus::Tab(id) => id.clone(),
                Focus::DevTools(id) => format!("devtools:{id}"),
            },
            tabs,
            zombies: 0,
            downloads: h.downloads.len(),
            permissions: h.permissions.len(),
        })
    })
    .flatten()
}

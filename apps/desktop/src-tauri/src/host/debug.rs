//! Read-only layout snapshots for the end-to-end test script.

use serde::Serialize;

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

/// Read-only snapshot of one window's webview placement, used by the end-to-end test script.
pub fn debug_layout(label: &str) -> Option<DebugLayout> {
    with_host(|h| {
        let scale = h.scale(label);
        let win = h.wins.get(label)?;
        let mut tabs: Vec<DebugTab> = h
            .tabs
            .iter()
            .filter(|(_, t)| t.window() == label)
            .map(|(id, t)| {
                let b = t.view.bounds().ok();
                let (x, y, w, hh) = b
                    .map(|b| {
                        let p = b.position.to_logical::<f64>(scale);
                        let s = b.size.to_logical::<f64>(scale);
                        (p.x, p.y, s.width, s.height)
                    })
                    .unwrap_or_default();
                DebugTab {
                    id: id.clone(),
                    target_id: t.shared.target_id.borrow().clone(),
                    requests: t.shared.requests.get(),
                    blocked: t.shared.blocked.get(),
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
            },
            tabs,
            zombies: h.zombies.len(),
            downloads: h.downloads.len(),
            permissions: h.permissions.len(),
        })
    })
    .flatten()
}

//! Zenith extension bridge: authoritative Space/split/Glance/Essentials tab model (per
//! `cef-extension-contract.md`), stable integer ID allocation, permission redaction, action
//! badges, and direct extension-tab adoption.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, RwLock};

use serde::{Deserialize, Serialize};

pub static BRIDGE: LazyLock<RwLock<ExtensionBridge>> =
    LazyLock::new(|| RwLock::new(ExtensionBridge::default()));

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TabSnapshot {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub favicon: String,
    #[serde(default)]
    pub loading: bool,
    #[serde(default)]
    pub discarded: bool,
    #[serde(default)]
    pub audible: bool,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub essential: bool,
    #[serde(default)]
    pub space_id: String,
    #[serde(default)]
    pub glance: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WindowSnapshot {
    pub label: String,
    #[serde(default)]
    pub incognito: bool,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub active_space_id: String,
    #[serde(default)]
    pub active_tab_id: Option<String>,
    /// Visible panes in the active split group, in pane order.
    #[serde(default)]
    pub visible_ids: Vec<String>,
    /// Ordered list of Space IDs in this window.
    #[serde(default)]
    pub space_order: Vec<String>,
    /// All tabs in this window (Essentials, pinned, regular, Glance).
    #[serde(default)]
    pub tabs: Vec<TabSnapshot>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChromeTab {
    pub id: i32,
    pub zenith_id: String,
    pub window_id: i32,
    pub index: usize,
    pub active: bool,
    pub highlighted: bool,
    pub hidden: bool,
    pub pinned: bool,
    pub discarded: bool,
    pub audible: bool,
    pub muted: bool,
    pub incognito: bool,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fav_icon_url: Option<String>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TabQuery {
    pub active: Option<bool>,
    pub hidden: Option<bool>,
    pub highlighted: Option<bool>,
    pub current_window: Option<bool>,
    pub last_focused_window: Option<bool>,
    pub window_id: Option<i32>,
    pub pinned: Option<bool>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub status: Option<String>,
    pub discarded: Option<bool>,
    pub audible: Option<bool>,
    pub muted: Option<bool>,
    pub index: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActionBadge {
    pub text: String,
    #[serde(default)]
    pub background_color: Option<String>,
    #[serde(default)]
    pub text_color: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BadgeEvent {
    pub extension_id: String,
    pub tab_id: Option<String>,
    pub badge: ActionBadge,
}

#[derive(Default)]
pub struct ExtensionBridge {
    next_tab_id: i32,
    next_win_id: i32,
    tab_ids: HashMap<String, i32>,
    win_ids: HashMap<String, i32>,
    windows: HashMap<String, WindowSnapshot>,
    /// Process-level focus recency (most recently focused window label at the back).
    focus_order: Vec<String>,
    /// Action badges keyed by `(extension_id, optional tab_id)`.
    badges: HashMap<(String, Option<String>), ActionBadge>,
}

impl ExtensionBridge {
    fn tab_int_id(&mut self, zenith_id: &str) -> i32 {
        if let Some(&id) = self.tab_ids.get(zenith_id) {
            return id;
        }
        self.next_tab_id += 1;
        let id = self.next_tab_id;
        self.tab_ids.insert(zenith_id.to_string(), id);
        id
    }

    fn win_int_id(&mut self, label: &str) -> i32 {
        if let Some(&id) = self.win_ids.get(label) {
            return id;
        }
        self.next_win_id += 1;
        let id = self.next_win_id;
        self.win_ids.insert(label.to_string(), id);
        id
    }

    pub fn sync_window(&mut self, snap: WindowSnapshot) {
        let label = snap.label.clone();
        self.win_int_id(&label);
        for t in &snap.tabs {
            if !t.glance {
                self.tab_int_id(&t.id);
            }
        }
        if snap.focused {
            self.focus_order.retain(|l| l != &label);
            self.focus_order.push(label.clone());
        } else if !self.focus_order.contains(&label) {
            self.focus_order.insert(0, label.clone());
        }
        self.windows.insert(label, snap);
    }

    pub fn drop_window(&mut self, label: &str) {
        self.windows.remove(label);
        self.focus_order.retain(|l| l != label);
    }

    pub fn set_badge(&mut self, extension_id: String, tab_id: Option<String>, badge: ActionBadge) -> BadgeEvent {
        let clean = ActionBadge {
            text: badge.text.chars().take(8).collect(),
            background_color: badge.background_color,
            text_color: badge.text_color,
        };
        if clean.text.is_empty() && clean.background_color.is_none() && clean.text_color.is_none() {
            self.badges.remove(&(extension_id.clone(), tab_id.clone()));
        } else {
            self.badges.insert((extension_id.clone(), tab_id.clone()), clean.clone());
        }
        BadgeEvent { extension_id, tab_id, badge: clean }
    }

    pub fn all_badges(&self) -> Vec<BadgeEvent> {
        self.badges
            .iter()
            .map(|((ext, tab), badge)| BadgeEvent {
                extension_id: ext.clone(),
                tab_id: tab.clone(),
                badge: badge.clone(),
            })
            .collect()
    }

    /// Queries tabs honoring `cef-extension-contract.md`:
    /// - Essentials first, then each Space in order (pinned before regular)
    /// - Glance excluded
    /// - `active` = focused split pane / activeTabId; `highlighted` = visible split panes
    /// - `hidden` = not visible in active split / non-active Space
    /// - `url`/`title`/`favIconUrl` redacted without `tabs` or matching host permission
    /// - Never crosses normal/incognito context boundary
    pub fn query_tabs(
        &self,
        caller_window: Option<&str>,
        caller_incognito: bool,
        has_tabs_perm: bool,
        host_permissions: &HashSet<String>,
        query: &TabQuery,
    ) -> Vec<ChromeTab> {
        let last_focused = self
            .focus_order
            .iter()
            .rev()
            .find(|l| self.windows.get(*l).is_some_and(|w| w.incognito == caller_incognito))
            .map(String::as_str);

        let mut out = Vec::new();
        for (label, win) in &self.windows {
            if win.incognito != caller_incognito {
                continue;
            }
            let Some(&win_id) = self.win_ids.get(label) else { continue };
            if query.current_window == Some(true) && caller_window != Some(label.as_str()) {
                continue;
            }
            if query.last_focused_window == Some(true) && last_focused != Some(label.as_str()) {
                continue;
            }
            if let Some(wanted_win) = query.window_id {
                if win_id != wanted_win {
                    continue;
                }
            }

            let ordered = flatten_tabs(win);
            let visible: HashSet<&str> = win.visible_ids.iter().map(String::as_str).collect();
            let active_id = win.active_tab_id.as_deref();

            for (index, tab) in ordered.into_iter().enumerate() {
                let Some(&tab_id) = self.tab_ids.get(&tab.id) else { continue };
                let active = active_id == Some(tab.id.as_str());
                let highlighted = if visible.is_empty() {
                    active
                } else {
                    visible.contains(tab.id.as_str())
                };
                let hidden = !highlighted;
                let pinned = tab.essential || tab.pinned;
                let status = if tab.loading { "loading" } else { "complete" };

                let can_read = has_tabs_perm || host_matches(&tab.url, host_permissions);
                let url = can_read.then(|| tab.url.clone());
                let title = can_read.then(|| tab.title.clone());
                let fav_icon_url = can_read.then(|| tab.favicon.clone()).filter(|s| !s.is_empty());

                if query.active.is_some_and(|v| v != active)
                    || query.highlighted.is_some_and(|v| v != highlighted)
                    || query.hidden.is_some_and(|v| v != hidden)
                    || query.pinned.is_some_and(|v| v != pinned)
                    || query.discarded.is_some_and(|v| v != tab.discarded)
                    || query.audible.is_some_and(|v| v != tab.audible)
                    || query.muted.is_some_and(|v| v != tab.muted)
                    || query.index.is_some_and(|v| v != index)
                    || query.status.as_deref().is_some_and(|s| s != status)
                {
                    continue;
                }
                // URL/title filters must never leak redacted matches.
                if let Some(want_url) = &query.url {
                    if !can_read || !url_matches(&tab.url, want_url) {
                        continue;
                    }
                }
                if let Some(want_title) = &query.title {
                    if !can_read || !tab.title.contains(want_title.as_str()) {
                        continue;
                    }
                }

                out.push(ChromeTab {
                    id: tab_id,
                    zenith_id: tab.id.clone(),
                    window_id: win_id,
                    index,
                    active,
                    highlighted,
                    hidden,
                    pinned,
                    discarded: tab.discarded,
                    audible: tab.audible,
                    muted: tab.muted,
                    incognito: win.incognito,
                    status,
                    url,
                    title,
                    fav_icon_url,
                });
            }
        }
        out.sort_by_key(|t| (t.window_id, t.index));
        out
    }

    /// Validates `tabs.move` constraints from `cef-extension-contract.md`:
    /// rejects cross-context moves, Essential cross-window moves, and cross-zone/Space jumps.
    pub fn validate_move(
        &self,
        zenith_tab_id: &str,
        dest_window: &str,
        dest_index: i32,
    ) -> Result<(), String> {
        let (src_label, src_win, tab) = self
            .windows
            .iter()
            .find_map(|(l, w)| {
                w.tabs
                    .iter()
                    .find(|t| t.id == zenith_tab_id && !t.glance)
                    .map(|t| (l.as_str(), w, t))
            })
            .ok_or("tab not found")?;
        let dst_win = self.windows.get(dest_window).ok_or("destination window not found")?;
        if src_win.incognito != dst_win.incognito {
            return Err("cannot move tab across normal and incognito windows".into());
        }
        if tab.essential && src_label != dest_window {
            return Err("cannot move Essential tab to another window".into());
        }
        if dest_index >= 0 && src_label == dest_window {
            let ordered = flatten_tabs(dst_win);
            let idx = (dest_index as usize).min(ordered.len().saturating_sub(1));
            if let Some(target) = ordered.get(idx) {
                let same_zone = tab.essential == target.essential
                    && tab.pinned == target.pinned
                    && (tab.essential || tab.space_id == target.space_id);
                if !same_zone {
                    return Err("target index crosses Essential, pinned, or Space boundary".into());
                }
            }
        }
        Ok(())
    }
}

/// Flattens a window's tabs in Zenith contract order: Essentials -> per-Space (pinned -> regular),
/// excluding ephemeral Glance surfaces.
fn flatten_tabs(win: &WindowSnapshot) -> Vec<&TabSnapshot> {
    let mut out = Vec::with_capacity(win.tabs.len());
    for t in win.tabs.iter().filter(|t| !t.glance && t.essential) {
        out.push(t);
    }
    let mut seen_spaces = HashSet::new();
    let mut spaces: Vec<&str> = win.space_order.iter().map(String::as_str).collect();
    for t in &win.tabs {
        if !t.glance && !t.essential && !spaces.contains(&t.space_id.as_str()) {
            spaces.push(&t.space_id);
        }
    }
    for sp in spaces {
        if !seen_spaces.insert(sp) {
            continue;
        }
        for t in win.tabs.iter().filter(|t| !t.glance && !t.essential && t.space_id == sp && t.pinned) {
            out.push(t);
        }
        for t in win.tabs.iter().filter(|t| !t.glance && !t.essential && t.space_id == sp && !t.pinned) {
            out.push(t);
        }
    }
    out
}

fn host_matches(url: &str, perms: &HashSet<String>) -> bool {
    if perms.contains("<all_urls>") || perms.contains("*://*/*") {
        return true;
    }
    let Some(host) = crate::platform::urls::host_of(url) else {
        return false;
    };
    perms.iter().any(|p| p.eq_ignore_ascii_case(host))
}

fn url_matches(url: &str, pattern: &str) -> bool {
    if pattern == "<all_urls>" || pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        url.starts_with(prefix)
    } else {
        url == pattern
    }
}

/// Adopts an extension-created browser (e.g. `chrome.tabs.create`, welcome/uninstall page) into
/// Zenith's primary window and closes the detached Chrome popup cleanly.
pub fn adopt_external_url(url: String) {
    if url.is_empty() || url == "about:blank" {
        return;
    }
    let target_win = crate::host::primary_window();
    crate::host::emit_to_window(
        &target_win,
        "tab-open-request",
        serde_json::json!({
            "url": url,
            "openerId": serde_json::Value::Null,
            "kind": "foreground",
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tab(id: &str, url: &str, space: &str, essential: bool, pinned: bool) -> TabSnapshot {
        TabSnapshot {
            id: id.into(),
            url: url.into(),
            title: format!("Title {id}"),
            favicon: String::new(),
            loading: false,
            discarded: false,
            audible: false,
            muted: false,
            pinned,
            essential,
            space_id: space.into(),
            glance: false,
        }
    }

    fn sample_bridge() -> ExtensionBridge {
        let mut b = ExtensionBridge::default();
        let mut glance = sample_tab("g1", "https://glance.test/", "s1", false, false);
        glance.glance = true;
        let mut sleeping = sample_tab("t3", "https://sleep.test/", "s2", false, false);
        sleeping.discarded = true;
        b.sync_window(WindowSnapshot {
            label: "main".into(),
            incognito: false,
            focused: true,
            active_space_id: "s1".into(),
            active_tab_id: Some("t2".into()),
            visible_ids: vec!["t1".into(), "t2".into()],
            space_order: vec!["s1".into(), "s2".into()],
            tabs: vec![
                sample_tab("t1", "https://one.test/", "s1", false, false),
                sample_tab("e1", "https://ess.test/", "", true, true),
                sample_tab("p1", "https://pin.test/", "s1", false, true),
                sample_tab("t2", "https://two.test/", "s1", false, false),
                sleeping,
                glance,
            ],
        });
        b.sync_window(WindowSnapshot {
            label: "incognito-1".into(),
            incognito: true,
            focused: false,
            active_space_id: "s1".into(),
            active_tab_id: Some("priv1".into()),
            visible_ids: vec!["priv1".into()],
            space_order: vec!["s1".into()],
            tabs: vec![sample_tab("priv1", "https://secret.test/", "s1", false, false)],
        });
        b
    }

    #[test]
    fn flattened_order_split_semantics_and_glance_exclusion() {
        let b = sample_bridge();
        let tabs = b.query_tabs(Some("main"), false, true, &HashSet::new(), &TabQuery::default());
        let ids: Vec<&str> = tabs.iter().map(|t| t.zenith_id.as_str()).collect();
        // Essentials -> Space 1 pinned -> Space 1 regular -> Space 2 regular; Glance excluded!
        assert_eq!(ids, vec!["e1", "p1", "t1", "t2", "t3"]);
        assert_eq!(tabs.iter().filter(|t| t.active).count(), 1);
        assert!(tabs[3].active, "focused split pane t2 is active");
        assert!(tabs[2].highlighted && tabs[3].highlighted, "both visible split panes highlighted");
        assert!(!tabs[2].hidden && !tabs[3].hidden);
        assert!(tabs[4].hidden && tabs[4].discarded, "hidden Space sleeping tab queryable & discarded");
    }

    #[test]
    fn permission_redaction_and_incognito_isolation() {
        let b = sample_bridge();
        let no_perm = b.query_tabs(Some("main"), false, false, &HashSet::new(), &TabQuery::default());
        assert!(no_perm.iter().all(|t| t.url.is_none() && t.title.is_none()));

        // URL filter must not leak redacted tabs.
        let leak = b.query_tabs(
            Some("main"),
            false,
            false,
            &HashSet::new(),
            &TabQuery { url: Some("https://one.test/*".into()), ..Default::default() },
        );
        assert!(leak.is_empty());

        // Incognito query sees only incognito tabs, never normal tabs.
        let priv_tabs = b.query_tabs(Some("incognito-1"), true, true, &HashSet::new(), &TabQuery::default());
        assert_eq!(priv_tabs.len(), 1);
        assert_eq!(priv_tabs[0].zenith_id, "priv1");
    }

    #[test]
    fn move_validation_enforces_zone_space_and_context_boundaries() {
        let b = sample_bridge();
        assert!(b.validate_move("t1", "main", 3).is_ok());
        assert!(b.validate_move("t1", "main", 0).is_err(), "cannot cross into Essentials");
        assert!(b.validate_move("t1", "main", 4).is_err(), "cannot cross into Space 2");
        assert!(b.validate_move("t1", "incognito-1", -1).is_err(), "cannot cross normal/incognito");
        assert!(b.validate_move("e1", "incognito-1", -1).is_err());
    }

    #[test]
    fn action_badges_clamp_and_clear() {
        let mut b = ExtensionBridge::default();
        let ev = b.set_badge(
            "ext1".into(),
            None,
            ActionBadge { text: "1234567890".into(), background_color: Some("#ff5500".into()), text_color: None },
        );
        assert_eq!(ev.badge.text, "12345678");
        assert_eq!(b.all_badges().len(), 1);
        b.set_badge("ext1".into(), None, ActionBadge::default());
        assert!(b.all_badges().is_empty());
    }
}

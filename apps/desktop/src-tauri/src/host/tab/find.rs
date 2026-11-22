//! Find in page via CEF `BrowserHost::find` / `stop_finding` and `FindHandler`.

use std::sync::Arc;

use cef::*;
use serde::Serialize;

use super::TabShared;
use crate::host::{emit_tab, with_host};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FindResult {
    id: String,
    matches: i32,
    active: i32,
}

wrap_find_handler! {
    pub(super) struct TabFindHandler {
        shared: Arc<TabShared>,
    }

    impl FindHandler {
        fn on_find_result(
            &self,
            _browser: Option<&mut Browser>,
            _identifier: ::std::os::raw::c_int,
            count: ::std::os::raw::c_int,
            _selection_rect: Option<&Rect>,
            active_match_ordinal: ::std::os::raw::c_int,
            _final_update: ::std::os::raw::c_int,
        ) {
            // Both CEF and Zenith's FindBar expect 1-based active match ordinal (-1 when none).
            let active = if count > 0 && active_match_ordinal > 0 {
                active_match_ordinal
            } else {
                -1
            };
            emit_tab(
                &self.shared.id,
                "find-result",
                FindResult { id: self.shared.id.clone(), matches: count, active },
            );
        }
    }
}

pub fn find(id: &str, op: &str, text: &str, case_sensitive: bool, _whole_word: bool) -> Result<(), String> {
    let target = with_host(|h| {
        let tab = h.tabs.get_mut(id)?;
        let browser = tab.browser.clone()?;
        if op == "start" {
            tab.find_text = text.to_string();
            tab.find_case = case_sensitive;
        }
        Some((browser, tab.find_text.clone(), tab.find_case))
    })
    .flatten();
    let (browser, query, match_case) = target.ok_or("find is unavailable for this tab")?;
    let host = browser.host().ok_or("browser host unavailable")?;
    let case_flag = if match_case { 1 } else { 0 };
    match op {
        "start" => {
            if query.is_empty() {
                host.stop_finding(1);
                emit_tab(id, "find-result", FindResult { id: id.to_string(), matches: 0, active: -1 });
            } else {
                // CEF passes `findNext` to Chromium as `find_match`; 0 would leave no active match.
                host.find(Some(&CefString::from(query.as_str())), 1, case_flag, 1);
            }
        }
        "next" => {
            if !query.is_empty() {
                host.find(Some(&CefString::from(query.as_str())), 1, case_flag, 1);
            }
        }
        "prev" => {
            if !query.is_empty() {
                host.find(Some(&CefString::from(query.as_str())), 0, case_flag, 1);
            }
        }
        _ => {
            host.stop_finding(1);
            emit_tab(id, "find-result", FindResult { id: id.to_string(), matches: 0, active: -1 });
        }
    }
    Ok(())
}

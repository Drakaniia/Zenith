//! Find in page (WebView2's find API with the default dialog suppressed).

use serde::Serialize;
use std::result::Result;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, HSTRING};

use crate::host::{emit_tab, with_host};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FindResult {
    id: String,
    matches: i32,
    active: i32,
}

pub fn find(id: &str, op: &str, text: &str, case_sensitive: bool, whole_word: bool) -> Result<(), String> {
    let prepared = with_host(|h| {
        let env = h.env.clone();
        let tab = h.tabs.get_mut(id)?;
        if tab.find.is_none() {
            let finder = unsafe { tab.core().cast::<ICoreWebView2_28>().and_then(|c| c.Find()) }.ok()?;
            let report = |finder: &ICoreWebView2Find, tab_id: &str| {
                let (mut matches, mut active) = (0i32, -1i32);
                unsafe {
                    let _ = finder.MatchCount(&mut matches);
                    let _ = finder.ActiveMatchIndex(&mut active);
                }
                emit_tab(tab_id, "find-result", FindResult { id: tab_id.to_string(), matches, active });
            };
            let mut token = 0i64;
            let tab_id = id.to_string();
            unsafe {
                let _ = finder.add_MatchCountChanged(
                    &FindMatchCountChangedEventHandler::create(Box::new(move |sender, _| {
                        if let Some(f) = sender {
                            report(&f, &tab_id);
                        }
                        Ok(())
                    })),
                    &mut token,
                );
                let tab_id = id.to_string();
                let _ = finder.add_ActiveMatchIndexChanged(
                    &FindActiveMatchIndexChangedEventHandler::create(Box::new(move |sender, _| {
                        if let Some(f) = sender {
                            report(&f, &tab_id);
                        }
                        Ok(())
                    })),
                    &mut token,
                );
            }
            tab.find = Some(finder);
        }
        Some((env, tab.find.clone()?))
    })
    .flatten();
    let (env, finder) = prepared.ok_or("find is unavailable for this tab")?;
    unsafe {
        match op {
            "start" => {
                let env15: ICoreWebView2Environment15 = env.cast().map_err(|e| e.to_string())?;
                let options = env15.CreateFindOptions().map_err(|e| e.to_string())?;
                let _ = options.SetFindTerm(&HSTRING::from(text));
                let _ = options.SetIsCaseSensitive(case_sensitive);
                let _ = options.SetShouldMatchWord(whole_word);
                let _ = options.SetSuppressDefaultFindDialog(true);
                let _ = options.SetShouldHighlightAllMatches(true);
                finder
                    .Start(&options, &FindStartCompletedHandler::create(Box::new(|_| Ok(()))))
                    .map_err(|e| e.to_string())?;
            }
            "next" => finder.FindNext().map_err(|e| e.to_string())?,
            "prev" => finder.FindPrevious().map_err(|e| e.to_string())?,
            _ => finder.Stop().map_err(|e| e.to_string())?,
        }
    }
    Ok(())
}

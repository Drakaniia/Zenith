//! The page context menu: "Download video/audio with Zenith" and Zenith's link commands.

use std::rc::Rc;

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use windows::Win32::System::Com::IStream;

use super::{is_web, take, OpenRequest, TabShared};
use crate::host::emit_tab;

pub(super) unsafe fn attach_context_menu(core: &ICoreWebView2, env: &ICoreWebView2Environment, s: Rc<TabShared>) {
    unsafe {
        let (Ok(c11), Ok(env9)) = (core.cast::<ICoreWebView2_11>(), env.cast::<ICoreWebView2Environment9>()) else {
            return;
        };
        let mut token = 0i64;
        let _ = c11.add_ContextMenuRequested(
            &ContextMenuRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let target = args.ContextMenuTarget()?;
                let items = args.MenuItems()?;
                // Videos and audio: open the media downloader for this element (also works for
                // streaming players whose source is a blob: address).
                let mut kind = COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND::default();
                target.Kind(&mut kind)?;
                if kind == COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_VIDEO || kind == COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_AUDIO {
                    let mut has_source = BOOL::default();
                    target.HasSourceUri(&mut has_source)?;
                    let source = if has_source.as_bool() {
                        let mut p = PWSTR::null();
                        target.SourceUri(&mut p)?;
                        take(p)
                    } else {
                        String::new()
                    };
                    let label = if kind == COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_VIDEO {
                        "Download video with Zenith…"
                    } else {
                        "Download audio with Zenith…"
                    };
                    let item = env9.CreateContextMenuItem(
                        &HSTRING::from(label),
                        None::<&IStream>,
                        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
                    )?;
                    let tab = s.id.clone();
                    let mut t = 0i64;
                    item.add_CustomItemSelected(
                        &CustomItemSelectedEventHandler::create(Box::new(move |_, _| {
                            crate::media::sniff::request_panel(&tab, &source);
                            Ok(())
                        })),
                        &mut t,
                    )?;
                    let separator = env9.CreateContextMenuItem(
                        &HSTRING::from(""),
                        None::<&IStream>,
                        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
                    )?;
                    items.InsertValueAtIndex(0, &item)?;
                    items.InsertValueAtIndex(1, &separator)?;
                }
                let mut has_link = BOOL::default();
                target.HasLinkUri(&mut has_link)?;
                if !has_link.as_bool() {
                    return Ok(());
                }
                let mut link = PWSTR::null();
                target.LinkUri(&mut link)?;
                let link = take(link);
                if !is_web(&link) {
                    return Ok(());
                }
                let mut count = 0u32;
                items.Count(&mut count)?;
                let mut index = 0u32;
                for i in 0..count {
                    let item = items.GetValueAtIndex(i)?;
                    let mut name = PWSTR::null();
                    item.Name(&mut name)?;
                    if take(name) == "openLinkInNewWindow" {
                        index = i + 1;
                        break;
                    }
                }
                for (label, kind) in [
                    ("Open link in new tab", "background"),
                    ("Open link in Glance", "glance"),
                    ("Open link in split view", "split"),
                    ("Open link in incognito window", "incognito"),
                ] {
                    let item = env9.CreateContextMenuItem(
                        &HSTRING::from(label),
                        None::<&IStream>,
                        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
                    )?;
                    let url = link.clone();
                    let opener = s.id.clone();
                    let mut t = 0i64;
                    item.add_CustomItemSelected(
                        &CustomItemSelectedEventHandler::create(Box::new(move |_, _| {
                            if kind == "incognito" {
                                crate::incognito::open_window(vec![url.clone()]);
                                return Ok(());
                            }
                            emit_tab(
                                &opener,
                                "tab-open-request",
                                OpenRequest { url: url.clone(), opener_id: Some(opener.clone()), kind: kind.into() },
                            );
                            Ok(())
                        })),
                        &mut t,
                    )?;
                    items.InsertValueAtIndex(index, &item)?;
                    index += 1;
                }
                Ok(())
            })),
            &mut token,
        );
    }
}

//! WebView2 event wiring for a tab. `wire` attaches the handlers group by group, in a fixed
//! order; new-window requests, permissions, web requests and the context menu live in their
//! own files.

use std::rc::Rc;

use base64::Engine as _;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use windows::Win32::System::Com::IStream;
use wry::WebViewExtWindows;

use super::context_menu::attach_context_menu;
use super::new_window::wire_new_window;
use super::permissions::wire_permissions;
use super::requests::attach_blocker;
use super::{is_web, take, TabPatch, TabRef, TabShared};
use crate::host::{config, db, emit_tab, post, with_win, Focus};
use crate::platform::urls::host_of;

/// Wires every WebView2 event Zenith cares about. Handlers capture `TabShared` instead of the
/// host so they keep working while the host is borrowed elsewhere.
pub(super) fn wire(view: &wry::WebView, core: &ICoreWebView2, env: &ICoreWebView2Environment, s: Rc<TabShared>) {
    let controller = view.controller();
    crate::host::attach_accelerators(&controller, Some(s.clone()), &s.window);
    unsafe {
        wire_focus_zoom(&controller, &s);
        wire_navigation(core, &s);
        wire_page_state(core, &s);
        wire_new_window(core, &s);
        wire_lifecycle(core, &s);
        wire_permissions(core, &s);
        attach_blocker(core, env, s.clone());
        attach_context_menu(core, env, s.clone());
    }
}

/// Controller events: focus and zoom.
unsafe fn wire_focus_zoom(controller: &ICoreWebView2Controller, s: &Rc<TabShared>) {
    let mut token = 0i64;
    unsafe {
        // Focus tracking (to restore the page focus after Alt+Tab).
        let sh = s.clone();
        let _ = controller.add_GotFocus(
            &FocusChangedEventHandler::create(Box::new(move |_, _| {
                let id = sh.id.clone();
                // May fire synchronously inside a layout pass; record it afterwards.
                let focused = id.clone();
                let window = sh.window.clone();
                post(move || {
                    with_win(&window, |w| w.focus = Focus::Tab(focused));
                });
                emit_tab(&id.clone(), "tab-focused", TabRef { id });
                Ok(())
            })),
            &mut token,
        );

        // Zoom changes made inside WebView2 (Ctrl+wheel, pinch). Zoom set by Zenith is reported
        // by `command("zoom")` itself.
        let sh = s.clone();
        if let Err(e) = controller.add_ZoomFactorChanged(
            &ZoomFactorChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(c) = sender {
                    let mut z = 1.0;
                    c.ZoomFactor(&mut z)?;
                    TabPatch { zoom: Some(z), ..TabPatch::new(&sh.id) }.send();
                }
                Ok(())
            })),
            &mut token,
        ) {
            eprintln!("[zenith] ZoomFactorChanged unavailable: {e}");
        }
    }
}

/// Navigation, address, history and title changes (history is recorded here).
unsafe fn wire_navigation(core: &ICoreWebView2, s: &Rc<TabShared>) {
    let mut token = 0i64;
    unsafe {
        let sh = s.clone();
        let _ = core.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let uri = take(uri);
                *sh.page_host.borrow_mut() = host_of(&uri).unwrap_or("").to_ascii_lowercase();
                *sh.nav_uri.borrow_mut() = uri;
                sh.blocked.set(0);
                sh.requests.set(0);
                sh.media.borrow_mut().reset();
                crate::media::sniff::notify(&sh);
                crate::host::autofill::on_navigation(&sh.id);
                TabPatch {
                    loading: Some(true),
                    blocked: Some(0),
                    crashed: Some(String::new()),
                    error: Some(String::new()),
                    ..TabPatch::new(&sh.id)
                }
                .send();
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(core) = sender else { return Ok(()) };
                let mut uri = PWSTR::null();
                core.Source(&mut uri)?;
                let uri = take(uri);
                *sh.url.borrow_mut() = uri.clone();
                TabPatch { url: Some(uri), ..TabPatch::new(&sh.id) }.send();
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_HistoryChanged(
            &HistoryChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(core) = sender else { return Ok(()) };
                let (mut back, mut fwd) = (BOOL::default(), BOOL::default());
                core.CanGoBack(&mut back)?;
                core.CanGoForward(&mut fwd)?;
                TabPatch {
                    can_go_back: Some(back.as_bool()),
                    can_go_forward: Some(fwd.as_bool()),
                    ..TabPatch::new(&sh.id)
                }
                .send();
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
                let (Some(core), Some(args)) = (sender, args) else { return Ok(()) };
                let mut ok = BOOL::default();
                args.IsSuccess(&mut ok)?;
                let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                let _ = args.WebErrorStatus(&mut status);
                let mut title = PWSTR::null();
                core.DocumentTitle(&mut title)?;
                let title = take(title);
                let url = sh.url.borrow().clone();
                let error = if ok.as_bool() || status == COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED {
                    String::new()
                } else {
                    format!("{}", status.0)
                };
                TabPatch {
                    loading: Some(false),
                    blocked: Some(sh.blocked.get()),
                    title: (!title.is_empty()).then(|| title.clone()),
                    error: Some(error),
                    ..TabPatch::new(&sh.id)
                }
                .send();
                if ok.as_bool() && is_web(&url) && config().record_history && !sh.private {
                    db().exec(move |c| {
                        let _ = crate::history::history_add_visit(c, &url, &title);
                    });
                }
                if ok.as_bool() {
                    if let Some((x, y)) = sh.restore_scroll.take() {
                        let js = format!("window.scrollTo({x}, {y})");
                        let _ = core.ExecuteScript(
                            &HSTRING::from(js),
                            &ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(()))),
                        );
                    }
                }
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_DocumentTitleChanged(
            &DocumentTitleChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(core) = sender else { return Ok(()) };
                let mut title = PWSTR::null();
                core.DocumentTitle(&mut title)?;
                let title = take(title);
                *sh.title.borrow_mut() = title.clone();
                let url = sh.url.borrow().clone();
                if is_web(&url) && !title.is_empty() && config().record_history && !sh.private {
                    let t = title.clone();
                    db().exec(move |c| {
                        let _ = crate::history::history_set_title(c, &url, &t);
                    });
                }
                TabPatch { title: Some(title), ..TabPatch::new(&sh.id) }.send();
                Ok(())
            })),
            &mut token,
        );
    }
}

/// Page state shown on the tab: favicon, audio, mute and fullscreen.
unsafe fn wire_page_state(core: &ICoreWebView2, s: &Rc<TabShared>) {
    let mut token = 0i64;
    unsafe {
        if let Ok(c15) = core.cast::<ICoreWebView2_15>() {
            let sh = s.clone();
            let _ = c15.add_FaviconChanged(
                &FaviconChangedEventHandler::create(Box::new(move |sender, _| {
                    let Some(core) = sender else { return Ok(()) };
                    let c15: ICoreWebView2_15 = core.cast()?;
                    let mut uri = PWSTR::null();
                    c15.FaviconUri(&mut uri)?;
                    if take(uri).is_empty() {
                        TabPatch { favicon: Some(String::new()), ..TabPatch::new(&sh.id) }.send();
                        return Ok(());
                    }
                    let id = sh.id.clone();
                    c15.GetFavicon(
                        COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG,
                        &GetFaviconCompletedHandler::create(Box::new(move |result, stream| {
                            if result.is_ok() {
                                if let Some(bytes) = stream.as_ref().and_then(read_stream) {
                                    if !bytes.is_empty() {
                                        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                                        TabPatch {
                                            favicon: Some(format!("data:image/png;base64,{data}")),
                                            ..TabPatch::new(&id)
                                        }
                                        .send();
                                    }
                                }
                            }
                            Ok(())
                        })),
                    )?;
                    Ok(())
                })),
                &mut token,
            );
        }

        if let Ok(c8) = core.cast::<ICoreWebView2_8>() {
            let sh = s.clone();
            let _ = c8.add_IsDocumentPlayingAudioChanged(
                &IsDocumentPlayingAudioChangedEventHandler::create(Box::new(move |sender, _| {
                    let Some(core) = sender else { return Ok(()) };
                    let c8: ICoreWebView2_8 = core.cast()?;
                    let mut playing = BOOL::default();
                    c8.IsDocumentPlayingAudio(&mut playing)?;
                    TabPatch { audible: Some(playing.as_bool()), ..TabPatch::new(&sh.id) }.send();
                    Ok(())
                })),
                &mut token,
            );
            let sh = s.clone();
            let _ = c8.add_IsMutedChanged(
                &IsMutedChangedEventHandler::create(Box::new(move |sender, _| {
                    let Some(core) = sender else { return Ok(()) };
                    let c8: ICoreWebView2_8 = core.cast()?;
                    let mut muted = BOOL::default();
                    c8.IsMuted(&mut muted)?;
                    TabPatch { muted: Some(muted.as_bool()), ..TabPatch::new(&sh.id) }.send();
                    Ok(())
                })),
                &mut token,
            );
        }

        let sh = s.clone();
        let _ = core.add_ContainsFullScreenElementChanged(
            &ContainsFullScreenElementChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(core) = sender else { return Ok(()) };
                let mut full = BOOL::default();
                core.ContainsFullScreenElement(&mut full)?;
                let id = sh.id.clone();
                let on = full.as_bool();
                post(move || crate::host::set_fullscreen_tab(&id, on));
                TabPatch { fullscreen: Some(on), ..TabPatch::new(&sh.id) }.send();
                Ok(())
            })),
            &mut token,
        );
    }
}

/// The page closing itself, and renderer crashes or hangs.
unsafe fn wire_lifecycle(core: &ICoreWebView2, s: &Rc<TabShared>) {
    let mut token = 0i64;
    unsafe {
        let sh = s.clone();
        let _ = core.add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                emit_tab(&sh.id, "tab-close-request", TabRef { id: sh.id.clone() });
                Ok(())
            })),
            &mut token,
        );

        let sh = s.clone();
        let _ = core.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                args.ProcessFailedKind(&mut kind)?;
                let state = match kind {
                    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => "crashed",
                    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => "unresponsive",
                    _ => return Ok(()),
                };
                TabPatch { crashed: Some(state.into()), loading: Some(false), ..TabPatch::new(&sh.id) }.send();
                Ok(())
            })),
            &mut token,
        );
    }
}

fn read_stream(stream: &IStream) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let mut read = 0u32;
        let hr = unsafe { stream.Read(buf.as_mut_ptr() as *mut _, buf.len() as u32, Some(&mut read)) };
        if hr.is_err() {
            return None;
        }
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
        if out.len() > 512 * 1024 {
            return None;
        }
    }
    Some(out)
}

//! Page context menu via CEF `ContextMenuHandler`: "Download video/audio with Zenith…" and
//! Zenith's link actions (background tab, Glance, split view, incognito window).

use std::sync::Arc;

use cef::*;

use super::{is_web, OpenRequest, TabShared};
use crate::host::cef_app::cef_str;
use crate::host::emit_tab;

const CMD_DOWNLOAD_MEDIA: i32 = 26500;
const CMD_LINK_BACKGROUND: i32 = 26501;
const CMD_LINK_GLANCE: i32 = 26502;
const CMD_LINK_SPLIT: i32 = 26503;
const CMD_LINK_INCOGNITO: i32 = 26504;

wrap_context_menu_handler! {
    pub(super) struct TabContextMenuHandler {
        shared: Arc<TabShared>,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            let (Some(params), Some(model)) = (params, model) else { return };
            let mtype = params.media_type();
            if mtype == ContextMenuMediaType::VIDEO || mtype == ContextMenuMediaType::AUDIO {
                let label = if mtype == ContextMenuMediaType::VIDEO {
                    "Download video with Zenith…"
                } else {
                    "Download audio with Zenith…"
                };
                model.insert_item_at(0, CMD_DOWNLOAD_MEDIA, Some(&CefString::from(label)));
                model.insert_separator_at(1);
            }

            let link = cef_str(params.link_url());
            if is_web(&link) {
                let mut idx = 0usize;
                for (cmd, label) in [
                    (CMD_LINK_BACKGROUND, "Open link in new tab"),
                    (CMD_LINK_GLANCE, "Open link in Glance"),
                    (CMD_LINK_SPLIT, "Open link in split view"),
                    (CMD_LINK_INCOGNITO, "Open link in incognito window"),
                ] {
                    model.insert_item_at(idx, cmd, Some(&CefString::from(label)));
                    idx += 1;
                }
                model.insert_separator_at(idx);
            }
        }

        fn on_context_menu_command(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            command_id: ::std::os::raw::c_int,
            _event_flags: EventFlags,
        ) -> ::std::os::raw::c_int {
            let Some(params) = params else { return 0 };
            match command_id {
                CMD_DOWNLOAD_MEDIA => {
                    let src = cef_str(params.source_url());
                    crate::media::sniff::request_panel(&self.shared.id, &src);
                    1
                }
                CMD_LINK_BACKGROUND | CMD_LINK_GLANCE | CMD_LINK_SPLIT | CMD_LINK_INCOGNITO => {
                    let link = cef_str(params.link_url());
                    if !is_web(&link) {
                        return 0;
                    }
                    if command_id == CMD_LINK_INCOGNITO {
                        crate::incognito::open_window(vec![link]);
                        return 1;
                    }
                    let kind = match command_id {
                        CMD_LINK_GLANCE => "glance",
                        CMD_LINK_SPLIT => "split",
                        _ => "background",
                    };
                    emit_tab(
                        &self.shared.id,
                        "tab-open-request",
                        OpenRequest {
                            url: link,
                            opener_id: Some(self.shared.id.clone()),
                            kind: kind.into(),
                        },
                    );
                    1
                }
                _ => 0,
            }
        }
    }
}

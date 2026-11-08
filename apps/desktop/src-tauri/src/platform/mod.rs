//! Windows platform plumbing shared by the domains: HTTPS GET over WinHTTP (`net`), web
//! address handling (`urls`) and the Windows shell, registry and memory services (`win`).

pub mod net;
pub mod urls;
pub mod win;

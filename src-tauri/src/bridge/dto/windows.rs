use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WindowState {
    pub is_minimized: bool,
    pub is_maximized: bool,
}

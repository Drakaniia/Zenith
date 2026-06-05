use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PageMetadata {
    pub title: String,
    pub favicon_url: Option<String>,
}

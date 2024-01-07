use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeroxideConfig {
    pub directories: Vec<String>,
    pub panel_domain: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SiteConfig {
    #[serde(default = "content_default")]
    pub content: String,
    #[serde(default = "db_default")]
    pub db_filename: String,
    #[serde(skip)]
    pub db_pool: Option<SqlitePool>,
    pub domain: String,
    pub routes: HashMap<String, PagePath>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PagePath {
    pub path: String,
    #[serde(default = "default_templated")]
    pub is_templated: bool,
}

fn default_templated() -> bool {
    false
}

fn content_default() -> String {
    "content".to_string()
}

fn db_default() -> String {
    "db.sqlite3".to_string()
}

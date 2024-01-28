use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tinytemplate_async::TinyTemplate;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeroxideConfig {
    pub directories: Vec<String>,
    pub panel_domain: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SiteConfig {
    #[serde(default = "content_default")]
    pub content: String,
    #[serde(default = "db_default")]
    pub db_filename: String,
    #[serde(skip)]
    pub db_pool: Option<SqlitePool>,
    pub domain: String,
    pub routes: HashMap<String, PagePath>,
    #[serde(skip_deserializing)]
    pub site_path: String,
    #[serde(skip)]
    pub templates: Arc<RwLock<TinyTemplate>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PagePath {
    pub path: String,
    #[serde(default = "default_template")]
    pub template: Option<String>,
}

fn default_template() -> Option<String> {
    None
}

fn content_default() -> String {
    "content".to_string()
}

fn db_default() -> String {
    "db.sqlite3".to_string()
}

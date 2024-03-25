use std::{
    collections::HashMap, fs, io, sync::{Arc, RwLock}
};

use axum::{
    extract::{Query, State},
    http::StatusCode,
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
    #[serde(default = "create_user_default")]
    pub create_user: bool,
}

impl SiteConfig {
    pub fn save(&self) -> io::Result<()> {
        let new_config = toml::to_string(&self).expect("Decoding the SiteConfig struct");
        fs::write(format!("{}/PeroxideSite.toml", self.site_path), new_config)
    }
}

fn create_user_default() -> bool {
    false
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

fn db_default() -> String {
    "db.sqlite3".to_string()
}

#[derive(Serialize, Deserialize)]
struct ChangeDomainReq {
    domain: String,
}

#[axum::debug_handler]
pub async fn change_domain(
    State(mut state): State<SiteConfig>,
    Query(req): Query<ChangeDomainReq>,
) -> StatusCode {
    state.domain = req.domain;
    match state.save() {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            log::error!("{e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

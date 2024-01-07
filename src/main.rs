use axum::routing::post;
use axum::{
    extract::{Form, Query, State},
    http::{StatusCode, Uri},
    response::Html,
    routing::get,
    Router,
};
use log::error;
use peroxide::auth::sign_in;
use peroxide::auth::sign_up;
use peroxide::config::{PeroxideConfig, SiteConfig};
use peroxide::post::{create_post, delete_post, get_post, Post};
use peroxide::site::init_site;
use serde::{Deserialize, Serialize};
use sqlx::{
    query, query_as,
    sqlite::{SqlitePool, SqlitePoolOptions},
    Row,
};
use std::{collections::HashMap, fs};
use tinytemplate::{format_unescaped, TinyTemplate};

use comrak::{markdown_to_html, Options};
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    femme::start();
    let config: PeroxideConfig =
        toml::from_str(fs::read_to_string("Peroxide.toml").unwrap().as_str()).unwrap();
    let mut work_group = tokio::task::JoinSet::new();
    for dir in config.directories.into_iter() {
        work_group.spawn(init_site(dir));
    }
    while let Some(result) = work_group.join_next().await {
        error!("{:?}", result);
    }
}

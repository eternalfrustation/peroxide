#![feature(exact_size_is_empty)]

use axum::handler::Handler;
use chrono::Utc;
use log::error;

use clap::Parser;

use peroxide::{config::PeroxideConfig, wordpress::WordpressSite};

use peroxide::site::init_site;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tower_http::follow_redirect::policy::PolicyExt;

use std::{borrow::BorrowMut, collections::HashMap, fmt::Debug, fs, str::FromStr};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(short, long, default_value_t = String::from("Peroxide.toml"))]
    config: String,
    wordpress_import: Option<String>,
    #[arg(short, long, default_value_t = String::from("./"))]
    wordpress_import_path: String,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let args = Args::parse();
    match args.wordpress_import {
        Some(site) => {
            let wp_site = WordpressSite::from_site_url(site).await;
            wp_site.save(args.wordpress_import_path);
        }
        None => {
            let config: PeroxideConfig =
                toml::from_str(fs::read_to_string(args.config).unwrap().as_str()).unwrap();
            let mut work_group = tokio::task::JoinSet::new();
            for dir in config.directories.into_iter() {
                work_group.spawn(init_site(dir));
            }
            while let Some(result) = work_group.join_next().await {
                error!("{:?}", result);
            }
        }
    };
}

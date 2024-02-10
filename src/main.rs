#![feature(exact_size_is_empty)]

use axum::handler::Handler;
use chrono::Utc;
use log::error;

use clap::Parser;

use peroxide::config::PeroxideConfig;

use peroxide::site::init_site;
use serde::{Deserialize, Serialize};
use tower_http::follow_redirect::policy::PolicyExt;

use std::{collections::HashMap, fs, str::FromStr};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(short, long, default_value_t = String::from("Peroxide.toml"))]
    config: String,
    wordpress_import: Option<String>,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let args = Args::parse();
    match args.wordpress_import {
        Some(site) => import_from_wordpress(site).await,
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

#[derive(Deserialize)]
struct WordpressQueryResp {
    id: u32,
    #[serde(with = "wordpress_date_format")]
    date: chrono::DateTime<Utc>,
    #[serde(with = "wordpress_date_format")]
    date_gmt: chrono::DateTime<Utc>,
    guid: Content,
    #[serde(with = "wordpress_date_format")]
    modified: chrono::DateTime<Utc>,
    #[serde(with = "wordpress_date_format")]
    modified_gmt: chrono::DateTime<Utc>,
    slug: String,
    status: WordpressStatus,
    #[serde(rename = "type")]
    page_type: WordpressRespType,
    link: String,
    title: Content,
    content: Content,
    excerpt: Content,
    author: isize,
    featured_media: isize,
    parent: isize,
    menu_order: isize,
    comment_status: String,
    ping_status: String,
    template: String,
    meta: Meta,
    categories: Vec<isize>,
    tags: Vec<isize>,
    #[serde(rename = "_links")]
    links: HashMap<String, Vec<LinkHref>>,
}

#[derive(Serialize, Deserialize)]
struct LinkHref {
    href: String,
    embeddable: Option<bool>,
    count: Option<isize>,
    id: Option<isize>,
    name: Option<String>,
    templated: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct Meta {
    #[serde(rename = "_et_pb_use_builder")]
    et_pb_use_builder: String,
    #[serde(rename = "_et_pb_old_content")]
    et_pb_old_content: String,
    #[serde(rename = "_et_gb_content_width")]
    et_gb_content_width: String,
    footnotes: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Content {
    rendered: String,
    protected: Option<bool>,
}

#[derive(Serialize, Deserialize)]
enum WordpressStatus {
    Publish,
    Draft,
    Inherit,
    Other(String),
}

#[derive(Serialize, Deserialize)]
enum WordpressRespType {
    Post,
    Page,
    Attachment,
    Other(String),
}

impl From<String> for WordpressRespType {
    fn from(value: String) -> Self {
        match value.as_str() {
            "post" => Self::Post,
            "page" => Self::Page,
            "attachment" => Self::Attachment,
            x => Self::Other(String::from(x)),
        }
    }
}
impl From<String> for WordpressStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "publish" => Self::Publish,
            "draft" => Self::Draft,
            "inherit" => Self::Inherit,
            x => Self::Other(String::from(x)),
        }
    }
}

mod wordpress_date_format {
    use chrono::{DateTime, NaiveDateTime, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};
    const FORMAT: &'static str = "%Y-%M-%DT%H:%M:%S";
    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(FORMAT));
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let dt = NaiveDateTime::parse_from_str(&s, FORMAT).map_err(serde::de::Error::custom)?;
        Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
    }
}

#[derive(Deserialize)]
struct WordpressTags {
    id: isize,
    count: isize,
    description: String,
    link: String,
    name: String,
    slug: String,
    taxonomy: String,
    meta: Vec<String>,
    #[serde(rename = "_links")]
    links: Vec<LinkHref>,
}

#[derive(Deserialize)]
struct WordpressMedia {
    id: u32,
    #[serde(with = "wordpress_date_format")]
    date: chrono::DateTime<Utc>,
    #[serde(with = "wordpress_date_format")]
    date_gmt: chrono::DateTime<Utc>,
    guid: Content,
    #[serde(with = "wordpress_date_format")]
    modified: chrono::DateTime<Utc>,
    #[serde(with = "wordpress_date_format")]
    modified_gmt: chrono::DateTime<Utc>,
    slug: String,
    status: WordpressStatus,
    #[serde(rename = "type")]
    page_type: WordpressRespType,
    link: String,
    title: Content,
    author: isize,
    comment_status: String,
    ping_status: String,
    template: String,
    meta: Meta,
    description: Content,
    caption: Content,
    alt_text: String,
    media_type: String,
    mime_type: String,
    post: isize,
    source_url: String,
    #[serde(rename = "_links")]
    links: Vec<LinkHref>,
}

async fn import_from_wordpress(site: String) {
    let post_req = reqwest::Request::new(
        reqwest::Method::GET,
        reqwest::Url::from_str(format!("{site}/wp-json/wp/v2/posts").as_str()).unwrap(),
    );
    let page_req = reqwest::Request::new(
        reqwest::Method::GET,
        reqwest::Url::from_str(format!("{site}/wp-json/wp/v2/pages").as_str()).unwrap(),
    );
    let tags_req = reqwest::Request::new(
        reqwest::Method::GET,
        reqwest::Url::from_str(format!("{site}/wp-json/wp/v2/tags").as_str()).unwrap(),
    );
    let media_req = reqwest::Request::new(
        reqwest::Method::GET,
        reqwest::Url::from_str(format!("{site}/wp-json/wp/v2/media").as_str()).unwrap(),
    );
    let client = reqwest::Client::new();
    let pages: Vec<WordpressQueryResp> = serde_json::from_slice(
        client
            .execute(page_req)
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .to_vec()
            .as_slice(),
    )
    .unwrap();

    let posts: Vec<WordpressQueryResp> = serde_json::from_slice(
        client
            .execute(post_req)
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .to_vec()
            .as_slice(),
    )
    .unwrap();

    let tags: Vec<WordpressTags> = serde_json::from_slice(
        client
            .execute(tags_req)
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .to_vec()
            .as_slice(),
    )
    .unwrap();
    let media: Vec<WordpressMedia> = serde_json::from_slice(
        client
            .execute(media_req)
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .to_vec()
            .as_slice(),
    )
    .unwrap();
}

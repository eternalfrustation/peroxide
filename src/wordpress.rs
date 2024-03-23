use std::str::FromStr;
use std::sync::RwLock;
use std::{collections::HashMap, sync::Arc};

use axum::http::Uri;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tinytemplate_async::TinyTemplate;

use crate::config::SiteConfig;

#[derive(Debug)]
pub struct WordpressSite {
    url: String,
    post: Vec<WordpressData>,
    page: Vec<WordpressData>,
    tags: Vec<WordpressTags>,
    media: Vec<WordpressMedia>,
    users: Vec<WordpressUser>,
    comments: Vec<WordpressComment>,
}

#[derive(Debug, Deserialize)]
struct WordpressComment {
    id: isize,
    post: isize,
    parent: isize,
    author: isize,
    author_name: String,
    author_url: String,
    date: String,
    date_gmt: String,
    content: Content,
    link: String,
    status: WordpressStatus,
    #[serde(rename = "type")]
    data_type: String,
    meta: Vec<Meta>,
    #[serde(rename = "_links")]
    links: TagLinks,
}

struct WordpressMediaUrls {}

#[derive(Deserialize, Debug)]
struct WordpressData {
    id: usize,
    date: String,
    date_gmt: String,
    guid: Content,
    modified: String,
    modified_gmt: String,
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
    comment_status: String,
    ping_status: String,
    template: String,
    meta: Meta,
    parent: Option<isize>,
    categories: Option<Vec<isize>>,
    tags: Option<Vec<isize>>,
    #[serde(rename = "_links")]
    links: HashMap<String, Vec<LinkHref>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct LinkHref {
    href: String,
    embeddable: Option<bool>,
    count: Option<isize>,
    id: Option<isize>,
    name: Option<String>,
    templated: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Meta {
    #[serde(rename = "_et_pb_use_builder")]
    et_pb_use_builder: String,
    #[serde(rename = "_et_pb_old_content")]
    et_pb_old_content: String,
    #[serde(rename = "_et_gb_content_width")]
    et_gb_content_width: String,
    footnotes: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Content {
    rendered: String,
    protected: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum WordpressStatus {
    #[serde(rename_all = "lowercase")]
    Publish,
    Draft,
    Inherit,
    Approved,
    Other(String),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum WordpressRespType {
    Post,
    Page,
    Attachment,
    Comment,
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
    use serde::{Deserialize, Deserializer, Serializer};
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

#[derive(Deserialize, Debug)]
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
    links: TagLinks,
}

#[derive(Deserialize, Debug)]
struct TagLinks {
    #[serde(rename = "self")]
    inner: Vec<LinkHref>,
}

pub enum SiteSaveError {
    DirectoryCreateError,
    DirectoryReadError,
    InvalidUrl,
}

impl WordpressSite {
    pub fn save(&self, path: String) -> Result<SiteConfig, SiteSaveError> {
        println!("{path}");
        let url: &str = self.url.as_str();
        std::fs::create_dir_all(path.clone()).map_err(|_| SiteSaveError::DirectoryCreateError)?;
        let domain = Uri::from_str(url)
            .map_err(|_| SiteSaveError::InvalidUrl)?
            .host()
            .unwrap()
            .to_string();
        let routes = HashMap::new();
        let templates = TinyTemplate::new();
        for _page in self.page.iter() {}
        let templates = Arc::from(RwLock::from(templates));
        Ok(SiteConfig {
            db_filename: "db.sqlite3".into(),
            db_pool: None,
            site_path: domain.clone(),
            domain,
            routes,
            templates,
        })
    }
}

impl WordpressSite {
    pub async fn from_site_url(url: String) -> WordpressSite {
        let client = Client::new();
        WordpressSite {
            url: url.clone(),
            post: serde_json::from_str(
                client
                    .get(
                        reqwest::Url::from_str(format!("{url}/wp-json/wp/v2/posts").as_str())
                            .unwrap(),
                    )
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
                    .as_str(),
            )
            .unwrap(),
            page: serde_json::from_str(
                client
                    .get(
                        reqwest::Url::from_str(format!("{url}/wp-json/wp/v2/pages").as_str())
                            .unwrap(),
                    )
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
                    .as_str(),
            )
            .unwrap(),
            tags: serde_json::from_str(
                client
                    .get(
                        reqwest::Url::from_str(format!("{url}/wp-json/wp/v2/tags").as_str())
                            .unwrap(),
                    )
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
                    .as_str(),
            )
            .unwrap(),
            media: serde_json::from_str(
                client
                    .get(
                        reqwest::Url::from_str(format!("{url}/wp-json/wp/v2/media").as_str())
                            .unwrap(),
                    )
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
                    .as_str(),
            )
            .unwrap(),
            users: serde_json::from_str(
                client
                    .get(
                        reqwest::Url::from_str(format!("{url}/wp-json/wp/v2/users").as_str())
                            .unwrap(),
                    )
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
                    .as_str(),
            )
            .unwrap(),
            comments: serde_json::from_str(
                client
                    .get(
                        reqwest::Url::from_str(format!("{url}/wp-json/wp/v2/comments").as_str())
                            .unwrap(),
                    )
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
                    .as_str(),
            )
            .unwrap(),
        }
    }
}

#[derive(Deserialize, Debug)]
struct WordpressUser {
    id: isize,
    name: String,
    slug: String,
}

#[derive(Deserialize, Debug)]
struct WordpressMedia {
    id: u32,
    date: String,
    date_gmt: String,
    guid: Content,
    modified: String,
    modified_gmt: String,
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
    links: TagLinks,
}

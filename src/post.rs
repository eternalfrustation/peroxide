use std::{borrow::Cow, error::Error, fs, hash::Hash, io::Write};

use axum::{
    extract::{FromRef, Query, State},
    http::StatusCode,
    response::{Html, Redirect},
    Form,
};
use axum_extra::handler::HandlerCallWithExtractors;
use comrak::Options;
use log::error;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::{
    database::HasValueRef,
    prelude::{FromRow, Type},
    query, query_as,
    sqlite::SqliteTypeInfo,
    Database, Decode, Encode, Sqlite, Value, ValueRef,
};
use tinytemplate_async::{format_unescaped, TinyTemplate};

use crate::{auth::User, config::SiteConfig};

#[derive(Serialize, Deserialize)]
pub struct PostCreateRequest {
    name: String,
    content: String,
    #[serde(default)]
    tags: Option<VecStr>,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct VecStr {
    pub data: Vec<String>,
}

impl Type<Sqlite> for VecStr {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <&[u8] as Type<Sqlite>>::type_info()
    }

    fn compatible(ty: &SqliteTypeInfo) -> bool {
        <&[u8] as Type<Sqlite>>::compatible(ty) || <Vec<u8> as Type<Sqlite>>::compatible(ty)
    }
}

impl<'r> Encode<'r, Sqlite> for VecStr
where
    &'r [u8]: Encode<'r, Sqlite>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::database::HasArguments<'r>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull {
        if self.data.is_empty() {
            return sqlx::encode::IsNull::Yes;
        }
        let mut bytes = Vec::new();
        for string in self.data.iter() {
            bytes
                .write_all(&(string.as_bytes().len() as u64).to_le_bytes())
                .unwrap();
            bytes.write_all(string.as_bytes()).unwrap();
        }
        let byte_slice: Box<[u8]> = Box::from(bytes.as_slice());
        <Box<[u8]> as Encode<Sqlite>>::encode(byte_slice, buf)
    }
}
// DB is the database driver
// `'r` is the lifetime of the `Row` being decoded
impl<'r, DB: Database> Decode<'r, DB> for VecStr
where
    Vec<u8>: Decode<'r, DB>,
{
    fn decode(
        value: <DB as HasValueRef<'r>>::ValueRef,
    ) -> Result<VecStr, Box<dyn Error + 'static + Send + Sync>> {
        // Decoding from a Rows of String format
        let ros = <Vec<u8> as Decode<DB>>::decode(value)?;
        let mut result: Vec<String> = Vec::new();
        let mut idx = 0;
        while idx < ros.len() {
            let length = u64::from_le_bytes(match ros.get(idx..(idx + 8)) {
                Some(x) => x.try_into().unwrap(),
                None => {
                    return Err(Box::new(sqlx::Error::TypeNotFound {
                        type_name: "Row of String".to_string(),
                    }))
                }
            });
            idx += 8;
            result.push(
                String::from_utf8(match ros.get(idx..(idx + length as usize)) {
                    Some(x) => x.to_vec(),
                    None => {
                        return Err(Box::new(sqlx::Error::TypeNotFound {
                            type_name: "Row of String".to_string(),
                        }))
                    }
                })
                .unwrap(),
            );
            idx += length as usize;
        }
        Ok(VecStr { data: result })
    }
}

impl From<Vec<u8>> for VecStr {
    fn from(ros: Vec<u8>) -> Self {
        // Decoding from a Rows of String format
        let mut result: Vec<String> = Vec::new();
        let mut idx = 0;
        while idx < ros.len() {
            let length = u64::from_le_bytes(match ros.get(idx..(idx + 8)) {
                Some(x) => x.try_into().unwrap(),
                None => {
                    log::warn!("Error while reading string length");
                    [0u8; 8]
                }
            });
            idx += 8;
            result.push(
                String::from_utf8(match ros.get(idx..(idx + length as usize)) {
                    Some(x) => x.to_vec(),
                    None => {
                        log::warn!("Error while reading string content");
                        Vec::new()
                    }
                })
                .unwrap(),
            );
            idx += length as usize;
        }
        VecStr { data: result }
    }
}

pub async fn create_post<'a>(
    State(config): State<SiteConfig>,
    user: User,
    form: Form<PostCreateRequest>,
) -> Result<Redirect, StatusCode> {
    let content = comrak::markdown_to_html(
        ammonia::clean(form.content.as_str()).as_str(),
        &Options::default(),
    );
    match query!(
        "INSERT INTO posts(name, content, tags, owner) VALUES(?1, ?2, ?3, ?4)",
        form.name,
        content,
        form.tags,
        user.username
    )
    .execute(&config.db_pool.clone().unwrap())
    .await
    {
        Err(e) => {
            error!("Error while inserting a post: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
        Ok(_) => Ok(Redirect::to("/admin?path=blogs")),
    }
}

#[derive(Serialize, Deserialize)]
pub struct PostDeleteRequest {
    id: i64,
}

pub async fn delete_post<'a>(
    State(config): State<SiteConfig>,
    user: User,
    form: Query<PostDeleteRequest>,
) -> Result<String, StatusCode> {
    match query!(
        "DELETE FROM posts WHERE id = ? AND owner = ?",
        form.id,
        user.username
    )
    .execute(&config.db_pool.clone().unwrap())
    .await
    {
        Ok(_) => Ok("Deleted".to_string()),
        Err(e) => {
            error!("Error while deleting a post: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PostGetRequest {
    #[serde(default = "one")]
    id: i64,
}

fn one() -> i64 {
    1
}

pub async fn get_post<'a>(
    query: Query<PostGetRequest>,
    State(config): State<SiteConfig>,
) -> Result<Html<String>, StatusCode> {
    match query_as!(
        Post,
        "SELECT id, name, content, date, tags, owner, status FROM posts WHERE id IS ?",
        query.id
    )
    .fetch_one(&config.db_pool.unwrap())
    .await
    {
        Ok(post) => {
            let template = &match config.templates.read() {
                Ok(t) => t,
                Err(e) => {
                    log::error!("Templates Lock poisoned: {e}");
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };
            match template.render("data/post", &post) {
                Ok(html) => Ok(Html(html)),
                Err(e) => {
                    log::error!("{e}");
                    Err(StatusCode::NOT_FOUND)
                }
            }
        }
        Err(e) => {
            log::warn!("{e}");
            Err(StatusCode::NOT_FOUND)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PostStatus {
    Draft,
    Published,
}

impl From<String> for PostStatus {
    fn from(value: String) -> Self {
        if value.eq("Published") {
            Self::Published
        } else {
            Self::Draft
        }
    }
}

#[derive(Serialize, FromRow, Deserialize, Clone, Debug)]
pub struct Post {
    pub id: i64,
    pub name: String,
    pub content: String,
    pub date: i64,
    pub tags: VecStr,
    pub owner: String,
    pub status: PostStatus,
}

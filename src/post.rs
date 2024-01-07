use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Html,
    Form,
};
use comrak::Options;
use log::error;
use serde::{Deserialize, Serialize};
use sqlx::{query, query_as};

use crate::{auth::User, config::SiteConfig};

#[derive(Serialize, Deserialize)]
pub struct PostCreateRequest {
    name: String,
    content: String,
    path: String,
}

pub async fn create_post(
    State(config): State<SiteConfig>,
    user: User,
    form: Form<PostCreateRequest>,
) -> Result<String, StatusCode> {
    let content = comrak::markdown_to_html(
        ammonia::clean(form.content.as_str()).as_str(),
        &Options::default(),
    );
    match query!(
        "INSERT INTO posts(name, content, path, owner) VALUES(?1, ?2, ?3, ?4)",
        form.name,
        content,
        form.path,
        user.username
    )
    .execute(&config.db_pool.clone().unwrap())
    .await
    {
        Err(e) => {
            error!("Error while inserting a post: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
        Ok(_) => Ok("Success".to_string()),
    }
}

#[derive(Serialize, Deserialize)]
pub struct PostDeleteRequest {
    id: i64,
}

pub async fn delete_post(
    State(config): State<SiteConfig>,
    user: User,
    form: Form<PostDeleteRequest>,
) -> Result<String, StatusCode> {
    match query!(
        "DELETE FROM posts WHERE id = ? AND owner = ?",
        form.id,
        user.username
    )
    .execute(&config.db_pool.clone().unwrap())
    .await
    {
        Ok(_) => Ok("Success".to_string()),
        Err(e) => {
            error!("Error while inserting a post: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PostGetRequest {
    id: i64,
}

pub async fn get_post(
    query: Query<PostGetRequest>,
    _user: User,
    State(config): State<SiteConfig>,
) -> Result<Html<String>, StatusCode> {
    match query_as!(
        Post,
        "SELECT id, name, content, date, path, owner FROM posts WHERE id IS ?",
        query.id
    )
    .fetch_one(&config.db_pool.unwrap())
    .await
    {
        Ok(post) => {
            let mut tt = tinytemplate::TinyTemplate::new();
            let full_path = format!("{}/{}", post.path, post.name);
            tt.add_template(full_path.as_str(), post.content.as_str())
                .unwrap();
            match tt.render(full_path.as_str(), &post) {
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
pub struct Post {
    pub id: i64,
    pub name: String,
    pub content: String,
    pub date: i64,
    pub path: String,
    pub owner: String,
}

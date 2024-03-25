use axum::{async_trait, extract::{FromRequestParts, Request}, http::{request::Parts, StatusCode}, middleware::Next, response::{IntoResponse, Response}, Extension};

use crate::config::SiteConfig;

use super::{sign_up::UserSignUp, user::{Rank, User}};

pub async fn create_privileged(user: UserSignUp, rank: Rank, state: &SiteConfig) -> Result<(), String> {
    let user: User = user.try_into().unwrap();
    let rank = rank.to_string();
    match &state.db_pool {
        Some(pool) => {
            match sqlx::query!("insert into users (name, username, profile_pic, salt, sh_pass, email, rank) values($1, $2, $3, $4, $5, $6, $7)", user.name, user.username, user.profile_pic, user.salt, user.sh_pass, user.email, rank).execute(pool).await {
                Err(e) => {
                    log::error!("{:?}", e);
                    Err(e.to_string())
                }
                Ok(r) => {
                    log::info!("{:?}", r);
                    Ok(())

                }
            }
        }
        None => {
            Err("DB not connected".to_string())
        }
    }
}

pub struct Admin(User);

#[async_trait]
impl<S> FromRequestParts<S> for Admin
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        use axum::RequestPartsExt;
        let Extension(user) = parts.extract::<Extension<User>>()
            .await
            .map_err(|err| err.into_response())?;

        if user.rank == Rank::Admin {
            Err(StatusCode::UNAUTHORIZED.into_response())
        } else {
            Ok(Self(user))
        }
    }
}

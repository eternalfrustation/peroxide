use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};

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

pub async fn admin_middleware(
    user: User,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if user.rank == Rank::Admin {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }

}

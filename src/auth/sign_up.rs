use axum::{extract::State, http::StatusCode, Json};
use axum_typed_multipart::{TryFromMultipart, TypedMultipart};
use clap::Parser;
use rand::random;
use serde::{Deserialize, Serialize};
use sha3::Sha3_512;
use sqlx::{prelude::FromRow, Encode};

use sha3::Digest;

use crate::config::SiteConfig;

use super::user::{Rank, User, UserInfo};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum UserSignUpError {
    FailedHashing,
}

impl TryFrom<UserSignUp> for User {
    type Error = UserSignUpError;

    fn try_from(value: UserSignUp) -> Result<Self, Self::Error> {
        let mut salt: [u8; 64] = [0; 64];
        for b in salt.iter_mut() {
            *b = random();
        }
        let mut hasher = Sha3_512::new();
        hasher.update(salt);
        hasher.update(value.pass);
        let salted_hash: Vec<u8> = hasher.finalize()[..].into();
        Ok(User {
            name: value.name,
            username: value.username,
            profile_pic: None,
            salt: Vec::from(salt),
            sh_pass: salted_hash,
            email: value.email,
            rank: Rank::User,
        })
    }
}

#[derive(Deserialize, Debug, Serialize, Encode, FromRow, TryFromMultipart, Clone)]
pub struct UserSignUp {
    pub name: String,
    pub username: String,
    pub pass: String,
    pub email: String,
}

pub async fn create_user(
    admin: User,
    State(state): State<SiteConfig>,
    TypedMultipart(user_resp): TypedMultipart<UserSignUp>,
) -> Result<Json<UserInfo>, StatusCode> {
    if admin.rank != Rank::Admin {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let new_user: User = user_resp.try_into().unwrap();
    match sqlx::query!("insert into users (name, username, profile_pic, salt, sh_pass, email) values($1, $2, $3, $4, $5, $6)", new_user.name, new_user.username, new_user.profile_pic, new_user.salt, new_user.sh_pass, new_user.email).execute(&state.db_pool.unwrap()).await {
        Err(e) => {
            log::error!("{:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
        Ok(r) => {
            log::info!("{:?}", r);
            Ok(Json(UserInfo {
                name: new_user.name,
                username: new_user.username,
                profile_pic: None,
                email: new_user.email,
                rank: new_user.rank,
            }))

        }
    }
}

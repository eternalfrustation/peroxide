use base64::Engine;
use once_cell::sync::Lazy;
use sha3::Digest;
use std::{fmt::Display, time::{Duration, SystemTime}};

use axum::{
    async_trait,
    extract::{FromRequestParts, Query, State},
    http::{request::Parts, StatusCode},
    Json,
};
use axum_extra::extract::cookie::Cookie;
use base64::engine::general_purpose;
use jsonwebtoken::{DecodingKey, EncodingKey, Validation};
use serde::{Deserialize, Serialize};
use sha3::Sha3_512;
use sqlx::query_as;

use crate::config::SiteConfig;

pub struct Keys {
    pub encoding: EncodingKey,
    pub decoding: DecodingKey,
}

// Keys for encoding JWTs
impl Keys {
    fn new(secret: &[u8]) -> Keys {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }
}

pub static KEYS: Lazy<Keys> = Lazy::new(|| {
    let secret = std::env::var("JWT_SECRET")
        .expect("JWT_SECRET Environment Vaiable not set, it must be set.");
    Keys::new(secret.as_bytes())
});

// User struct, maps to the database
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct User {
    pub name: String,
    pub username: String,
    pub profile_pic: Option<String>,
    pub salt: Vec<u8>,
    pub sh_pass: Vec<u8>,
    pub email: String,
    pub rank: Rank,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub enum Rank {
    #[default]
    User,
    Admin,
}

impl Display for Rank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match *self {
            Self::User => "User",
            Self::Admin => "Admin",
        })
    }
}

impl From<String> for Rank {
    fn from(value: String) -> Self {
        if value.eq("Admin") {
            Self::Admin
        } else {
            Self::User
        }
    }
}

// implementing the "Auto Auth thing", slap a User in the arguments to a handler
// and BAM, you get Auth
#[async_trait]
impl<'a> FromRequestParts<SiteConfig> for User {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SiteConfig,
    ) -> Result<Self, Self::Rejection> {
        // Grab the Cookies, if not found, send back error
        match parts.headers.get("Cookie") {
            Some(cookie_string) => match Cookie::split_parse(match cookie_string.to_str() {
                Ok(s) => s,
                Err(_) => return Err((StatusCode::BAD_REQUEST, "Invalid Cookies")),
            })
            .map(|cookie| match cookie {
                Ok(c) => Some(c),
                Err(_) => None,
            })
            .filter(|c| c.is_some())
            .map(|c| c.unwrap())
            // Check if any of them is a "jwt-token"
            .filter(|c| c.name() == "jwt-token")
            .next()
            {
                // Check id the jwt-token cookie is actually a jsonwebtoken,
                // and decode it
                Some(c) => match jsonwebtoken::decode::<UserToken>(
                    c.value(),
                    &KEYS.decoding,
                    &Validation::new(jsonwebtoken::Algorithm::HS256),
                ) {
                    // If it is a valid JWT, grab the User struct from the database
                    // and match its creds with the token, then, if they match,
                    // return the User struct
                    Ok(token) => {
                        // The database query
                        let user = match query_as!(
                            User,
                            "select * from users where username = ?",
                            token.claims.username
                        )
                        .fetch_one(&state.db_pool.clone().unwrap())
                        .await
                        {
                            // If found return the user
                            Ok(user) => user,
                            // If not, Yell through HTTP
                            Err(e) => {
                                log::error!("{}", e);
                                return Err((
                                    StatusCode::UNAUTHORIZED,
                                    "Incorrent Username or Password",
                                ));
                            }
                        };

                        // Hash the password to prepare for matching with the password in the db
                        let mut hasher = Sha3_512::new();

                        hasher.update(user.sh_pass.clone());

                        // read hash digest
                        let result: Vec<u8> = hasher.finalize()[..].into();

                        // Check the new password with the password in the db
                        // If both are the same, return the user
                        let resp_hash = match general_purpose::STANDARD.decode(token.claims.sh_pass)
                        {
                            Ok(h) => h,
                            Err(e) => {
                                log::error!("{}", e);
                                return Err((
                                    StatusCode::BAD_REQUEST,
                                    "Incorrect username or password",
                                ));
                            }
                        };
                        if result == resp_hash {
                            return Ok(user);
                        }
                        // else, Yell through HTTP
                        return Err((StatusCode::UNAUTHORIZED, "Incorrect username or password"));
                    }
                    Err(e) => {
                        log::error!("At line {}, {}", line!(), e);
                        return Err((StatusCode::BAD_REQUEST, "Could not parse the JWT"));
                    }
                },
                None => {
                    return Err((StatusCode::UNAUTHORIZED, "JWT Cookie not found"));
                }
            },
            None => return Err((StatusCode::UNAUTHORIZED, "JWT Cookie not found")).into(),
        };
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct UserToken {
    username: String,
    sh_pass: String,
    exp: u64,
}

impl From<User> for UserToken {
    fn from(user: User) -> Self {
        let mut hasher = Sha3_512::new();
        hasher.update(user.sh_pass);
        let shh_pass: Vec<u8> = hasher.finalize()[..].into();
        let exp_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            + Duration::from_secs(60 * 60 * 24);

        UserToken {
            username: user.username,
            sh_pass: general_purpose::STANDARD.encode(shh_pass),
            exp: exp_time.as_secs(),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
struct Claims {
    username: String,
    sh_pass: Vec<u8>,
    exp: usize,
}

#[derive(Serialize, Deserialize)]
pub struct UserInfo {
    pub name: String,
    pub username: String,
    pub profile_pic: Option<String>,
    pub email: String,
    pub rank: Rank,
}

#[derive(Serialize, Deserialize)]
pub struct UserGetRequest {
    pub username: String,
}

#[axum::debug_handler]
pub async fn get_user(
    Query(req): Query<UserGetRequest>,
    State(state): State<SiteConfig>,
) -> Result<Json<UserInfo>, StatusCode> {
    let username = req.username;
    match sqlx::query_as!(
        UserInfo,
        "SELECT name, username, profile_pic, email, rank FROM users WHERE username IS ?",
        username
    )
    .fetch_one(&state.db_pool.unwrap())
    .await
    {
        Ok(user) => Ok(Json(user)),
        Err(e) => {
            log::error!("{e}");
            Err(StatusCode::NOT_FOUND)
        }
    }
}

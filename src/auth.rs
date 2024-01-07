use std::time::{Duration, SystemTime};

use crate::config::SiteConfig;

use axum::{
    async_trait,
    extract::{FromRequestParts, State},
    http::{request::Parts, StatusCode},
    response::Redirect,
    Form,
};

use base64::{engine::general_purpose, Engine as _};

use rand::random;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_512};
use sqlx::{query_as, Encode, FromRow};

use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};

use once_cell::sync::Lazy;

use axum_extra::extract::{
    cookie::{Cookie, SameSite},
    CookieJar,
};

struct Keys {
    encoding: EncodingKey,
    decoding: DecodingKey,
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

static KEYS: Lazy<Keys> = Lazy::new(|| {
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
}

// implementing the "Auto Auth thing", slap a User in the arguments to a handler
// and BAM, you get Auth
#[async_trait]
impl FromRequestParts<SiteConfig> for User {
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
pub enum UserSignUpError {
    FailedHashing,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
enum UserSignInError {
    FailedHashing,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct UserToken {
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
        })
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
struct Claims {
    username: String,
    sh_pass: Vec<u8>,
    exp: usize,
}

#[derive(Deserialize, Serialize, Encode, FromRow)]
pub struct UserSignIn {
    username: String,
    pass: String,
}

pub async fn sign_in(
    cookie_jar: CookieJar,
    State(state): State<SiteConfig>,
    Form(user_resp): Form<UserSignIn>,
) -> Result<CookieJar, (StatusCode, String)> {
    let user = match query_as!(
        User,
        "select * from users where username = ?",
        user_resp.username
    )
    .fetch_one(&state.db_pool.unwrap())
    .await
    {
        Ok(user) => user,
        Err(e) => {
            log::error!("{}", e);
            return Err((StatusCode::UNAUTHORIZED, e.to_string()));
        }
    };
    let mut hasher = Sha3_512::new();
    hasher.update(user.salt.clone());
    hasher.update(user_resp.pass.as_bytes());
    let salted_hash: Vec<u8> = hasher.finalize()[..].into();
    if Vec::from(salted_hash) == user.sh_pass {
        let user_token: UserToken = user.into();
        match jsonwebtoken::encode(&Header::default(), &user_token, &KEYS.encoding) {
            Err(e) => {
                log::error!("{}", e);
                return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
            }
            Ok(s) => {
                return Ok(cookie_jar.add(
                    Cookie::build(("jwt-token", s))
                        .http_only(true)
                        .secure(true)
                        .same_site(SameSite::Lax)
                        .max_age(Duration::from_secs(60 * 60 * 12).try_into().unwrap())
                        .path("/"),
                ));
            }
        };
    }
    Err((
        StatusCode::UNAUTHORIZED,
        "Incorrect Username or password".to_string(),
    ))
}
#[derive(Deserialize, Serialize, Encode, FromRow)]
pub struct UserSignUp {
    name: String,
    username: String,
    pass: String,
    email: String,
}

pub async fn sign_up(
    cookie_jar: CookieJar,
    State(state): State<SiteConfig>,
    Form(user_resp): Form<UserSignUp>,
) -> Result<CookieJar, (StatusCode, String)> {
    let user: User = user_resp.try_into().unwrap();
    match sqlx::query!("insert into users (name, username, profile_pic, salt, sh_pass, email) values($1, $2, $3, $4, $5, $6)", user.name, user.username, user.profile_pic, user.salt, user.sh_pass, user.email).execute(&state.db_pool.unwrap()).await {
        Err(e) => {
            log::error!("{:?}", e);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, String::from("Could not Sign Up") + e.to_string().as_str()));
        }
        Ok(r) => {
            log::info!("{:?}", r);
            let user_token: UserToken = user.into();
            match jsonwebtoken::encode(&Header::default(), &user_token, &KEYS.encoding) {
                Err(e) => {
                    return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
                }
                Ok(s) => {
                    return Ok(cookie_jar
                        .add(Cookie::build(("jwt-token", s))
                            .http_only(true)
                            .secure(true)
                            .same_site(SameSite::None)
                            .max_age(Duration::from_secs(60 * 60 * 12).try_into().unwrap())
                            .path("/")
                        ));
                }
            };
        }
    };
}

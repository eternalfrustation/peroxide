
#[derive(Deserialize, Serialize, Clone, Debug)]
enum UserSignInError {
    FailedHashing,
}
#[derive(Deserialize, Serialize, Encode, FromRow, TryFromMultipart)]
pub struct UserSignIn {
    username: String,
    pass: String,
}

pub async fn sign_in<'a>(
    cookie_jar: CookieJar,
    State(state): State<SiteConfig>,
    TypedMultipart(user_resp): TypedMultipart<UserSignIn>,
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
            return Err((StatusCode::UNAUTHORIZED, String::from("Unable to Log in")));
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
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    String::from("Unable to Log in"),
                ));
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

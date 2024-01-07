async fn init_site(path: String) {
    let mut site_config: SiteConfig = match toml::from_str(
        match fs::read_to_string(format!("{}/PeroxideSite.toml", path)) {
            Ok(t) => t,
            Err(e) => {
                error!(
                    "Failed to read from the config file {}/PeroxideSite.toml, Error: {}",
                    path, e
                );
                return;
            }
        }
        .as_str(),
    ) {
        Ok(t) => t,
        Err(e) => {
            error!(
                "Failed to parse the config file {}/PeroxideSite.toml, Error: {}",
                path, e
            );
            return;
        }
    };
    let db_conn_url = format!("sqlite://{}/{}", path, site_config.db_filename);
    log::info!("Beginning Connection to {db_conn_url}");
    let pool: SqlitePool = {
        match SqlitePoolOptions::new()
            .max_connections(50)
            .connect(db_conn_url.as_str())
            .await
        {
            Ok(t) => t,
            Err(e) => {
                error!(
                    "Failed to connect to the sqlite database at {}, Error: {}",
                    db_conn_url, e
                );
                return;
            }
        }
    };

    match query!(
        "CREATE TABLE IF NOT EXISTS users(
        	salt blob unique not null,
        	name text not null,
        	username text unique not null primary key,
        	profile_pic text,
        	sh_pass blob not null,
        	email text not null unique
        ) STRICT"
    )
    .execute(&pool)
    .await
    {
        Ok(t) => t,
        Err(e) => {
            error!(
                "Failed to create the database users at {}, Error: {}",
                db_conn_url, e
            );
            return;
        }
    };

    match query!(
        "CREATE TABLE IF NOT EXISTS posts(
            id INTEGER NOT NULL PRIMARY KEY,
            name TEXT NOT NULL,
            content TEXT NOT NULL,
            date INTEGER NOT NULL DEFAULT (unixepoch(CURRENT_TIMESTAMP)),
            path TEXT NOT NULL,
            owner TEXT NOT NULL,
            FOREIGN KEY(owner) REFERENCES users(username)
        ) STRICT"
    )
    .execute(&pool)
    .await
    {
        Ok(t) => t,
        Err(e) => {
            error!(
                "Failed to create the database posts at {}, Error: {}",
                db_conn_url, e
            );
            return;
        }
    };
    site_config.db_pool = Some(pool);
    let app = setup_routes(&site_config);
    let listener = match tokio::net::TcpListener::bind(site_config.domain.clone()).await {
        Ok(t) => t,
        Err(e) => {
            error!(
                "Failed to bind to the address {}, Error: {}",
                site_config.domain.clone(),
                e
            );
            return;
        }
    };

    match axum::serve(listener, app).await {
        Ok(t) => t,
        Err(e) => {
            error!(
                "Error occured while serving the website {}, Error: {}",
                site_config.domain, e
            );
            return;
        }
    };
}

fn setup_routes(config: &SiteConfig) -> Router {
    Router::new()
        .nest(
            "/api",
            Router::new()
                .route("/post", get(get_post).post(create_post).delete(delete_post))
                .route("/sign_in", post(sign_in))
                .route("/sign_up", post(sign_up)),
        )
        .nest_service("/static", ServeDir::new("static"))
        .fallback(get(page_handler))
        .with_state(config.clone())
}

#[axum::debug_handler]
async fn page_handler(
    uri: Uri,
    State(config): State<SiteConfig>,
) -> Result<Html<String>, (StatusCode, Html<String>)> {
    let page_path = match config.routes.get(uri.path()) {
        Some(s) => s,
        None => {
            let slash_path = uri.path().rfind('/');
            let path = match slash_path {
                Some(idx) => &uri.path()[0..idx],
                None => "",
            };
            match config.routes.get(path) {
                Some(s) => s,
                None => {
                    return Err((
                        StatusCode::NOT_FOUND,
                        Html("<h1>Not found</h1>".to_string()),
                    ));
                }
            }
        }
    }
    .clone();
    match fs::read_to_string(format!("templates/{}", page_path.path)) {
        Ok(s) => {
            if !page_path.is_templated {
                return Ok(Html(s));
            }
            let name = uri.path().split('/').rev().next().unwrap();
            let slash_path = uri.path().rfind('/');
            let path = match slash_path {
                Some(idx) => &uri.path()[0..idx],
                None => "",
            };
            match query_as!(
                Post,
                "SELECT id, name, content, date, path, owner FROM posts WHERE path IS ? AND name IS ?",
                path,
                name
            )
            .fetch_one(&config.db_pool.unwrap())
            .await
            {
                Ok(post) => {
                    let mut tt: TinyTemplate<'_> = tinytemplate::TinyTemplate::new();
                    tt.set_default_formatter(&format_unescaped);
                    let full_path = format!("{}/{}", post.path, post.name);
                    tt.add_template(full_path.as_str(), s.as_str()).unwrap();
                    match tt.render(full_path.as_str(), &post) {
                        Ok(html) => Ok(Html(html)),
                        Err(e) => {
                            log::error!("{e}");
                            Err((StatusCode::NOT_FOUND, Html("Not Found".to_string())))
                        }
                    }
                }
                Err(e) => {
                    log::warn!("{e}");
                    Err((StatusCode::NOT_FOUND, Html(e.to_string())))
                }
            }
        }
        Err(e) => {
            log::error!("{}", e);
            Err((StatusCode::NOT_FOUND, Html(e.to_string())))
        }
    }
}

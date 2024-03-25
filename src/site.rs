use inquire::{Password, Select, Text};
use serde::*;
use std::{
    collections::HashMap,
    fmt::Write,
    fs,
    sync::{Arc, RwLock},
};

use axum::{
    extract::{Path, State},
    http::{StatusCode, Uri},
    response::Html,
    routing::{get, post},
    Router,
};
use log::error;
use sqlx::{query, query_as, sqlite::SqlitePoolOptions, SqlitePool};
use tinytemplate_async::TinyTemplate;
use tower_http::services::ServeDir;

use crate::{
    auth::{
        admin::{admin_middleware, create_privileged},
        sign_in::sign_in,
        sign_up::{create_user, UserSignUp},
        user::{get_user, Rank, User},
    },
    config::{PagePath, SiteConfig},
    post::{create_post, delete_post, get_post, Post},
};

pub async fn init_site(path: String) {
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
    site_config.site_path = path.clone();
    site_config.templates = Arc::from(RwLock::new(setup_templates(
        &site_config.routes,
        path.clone(),
    )));
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
        	email text not null unique,
            rank text not null
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
            tags BLOB NOT NULL DEFAULT X'',
            status TEXT NOT NULL DEFAULT 'Draft',
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
    if site_config.create_user {
        log::info!("Creating a user for the site: {}", site_config.site_path);
        site_config.create_user = false;
        let name = Text::new("Enter the display name: ")
            .with_help_message("This will be displayed to other people")
            .prompt()
            .unwrap();
        let username = Text::new("Enter the username: ")
            .with_help_message("This will be used for logging in")
            .prompt()
            .unwrap();
        let pass = Password::new("Enter the password: ").prompt().unwrap();
        let email = Text::new("Enter the mail: ").prompt().unwrap();
        let rank = Select::new("Select the rank: ", vec![Rank::Admin, Rank::User])
            .prompt()
            .unwrap();

        create_privileged(
            UserSignUp {
                name,
                username,
                pass,
                email,
            },
            rank,
            &site_config,
        )
        .await
        .unwrap();
        log::info!("Added user successfully");
        let new_config = toml::to_string(&site_config).unwrap();
        fs::write(format!("{}/PeroxideSite.toml", path), new_config).unwrap();
    }
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

fn increment(
    value: &serde_json::Value,
    string: &mut String,
) -> tinytemplate_async::error::Result<()> {
    let num = match value {
        serde_json::Value::Number(num) => num,
        a => {
            return Err(tinytemplate_async::error::Error::ParseError {
                msg: format!("Could not increment the non number input: {}", a).to_string(),
                line: 0,
                column: 0,
            })
        }
    };
    string.push_str((num.as_f64().unwrap() + 1.0).to_string().as_str());
    Ok(())
}

fn human_date(
    value: &serde_json::Value,
    string: &mut String,
) -> tinytemplate_async::error::Result<()> {
    let num = match value {
        serde_json::Value::Number(num) => num,
        a => {
            return Err(tinytemplate_async::error::Error::ParseError {
                msg: format!("Could not increment the non number input: {}", a).to_string(),
                line: 0,
                column: 0,
            })
        }
    };
    let post_time = chrono::DateTime::from_timestamp(num.as_i64().unwrap(), 0).unwrap();
    string
        .write_str(post_time.format("%H:%M:%S %d %b %Y").to_string().as_str())
        .unwrap();
    Ok(())
}
fn setup_templates(routes: &HashMap<String, PagePath>, site_path: String) -> TinyTemplate {
    let mut templates = TinyTemplate::new();
    templates.add_formatter("increment".to_string(), increment);
    templates.add_formatter("human_date".to_string(), human_date);
    let admin_template = fs::read_to_string("data/admin.templ.html").unwrap();
    templates
        .add_template("admin".to_string(), admin_template)
        .unwrap();
    for p in ["admin_panel/", "data/"].into_iter() {
        for ele in fs::read_dir(p).unwrap() {
            match ele {
                Ok(entry) => {
                    if !entry.path().is_file() {
                        continue;
                    }
                    let path = entry.path();
                    let content = fs::read_to_string(entry.path()).unwrap();
                    let path = match path.extension() {
                        Some(ext) => path
                            .to_str()
                            .unwrap()
                            .to_string()
                            .strip_suffix((String::from(".") + ext.to_str().unwrap()).as_str())
                            .unwrap()
                            .to_string(),
                        None => path.to_str().unwrap().to_string(),
                    };
                    println!("{:?}", path);
                    templates.add_template(path, content).unwrap();
                }
                Err(e) => {
                    log::error!("{}", e);
                    continue;
                }
            }
        }
    }

    for (name, path) in routes {
        log::info!("Found template file {name}");
        let content =
            fs::read_to_string(format!("{site_path}/templates/{}", path.path.clone())).unwrap();
        match templates.add_template(format!("pages{name}"), content) {
            Ok(t) => t,
            Err(e) => log::error!(
                "Error while compiling template: pages{name} with error: {}",
                e,
            ),
        };
        match path.template.clone() {
            Some(template_path) => {
                let content =
                    fs::read_to_string(format!("{site_path}/templates/{template_path}")).unwrap();
                templates
                    .add_template(format!("pages/{name}.templ"), content)
                    .unwrap();
            }
            None => {}
        }
    }
    templates
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AdminPageTempl {
    #[serde(default = "default_admin_page")]
    path: String,
    #[serde(skip_deserializing)]
    content: Option<String>,
    #[serde(skip_deserializing)]
    user: User,
}

fn default_admin_page() -> String {
    "blogs".to_string()
}

async fn serve_react(_user: User) -> Result<Html<String>, StatusCode> {
    fs::read_to_string("admin_panel/index.html")
        .map(|content| Html(content))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn setup_routes(config: &SiteConfig) -> Router {
    let router = Router::new()
        .nest(
            "/admin",
            Router::new()
                .nest_service("/assets", ServeDir::new("admin/assets"))
                .route("/", get(serve_react)),
        )
        .nest(
            "/api",
            Router::new()
                .route("/post", get(get_post).post(create_post).delete(delete_post))
                .route("/user", get(get_user).put(sign_in))
                .nest("/admin", Router::new().route("/user", post(create_user)).layer(admin_middleware)),
        )
        .nest_service(
            "/static",
            ServeDir::new(format!("{}/static", config.site_path)),
        );
    let mut site_router = Router::new();
    for (route, path) in config.routes.iter() {
        match path.template.clone() {
            Some(_) => {
                site_router = site_router.route(
                    format!("{}/:page", route).as_str(),
                    get(handle_page_templated),
                )
            }
            None => site_router = site_router.route(route, get(handle_page)),
        }
    }
    router.nest("", site_router).with_state(config.clone())
}

pub async fn handle_page_templated(
    uri: Uri,
    Path(page): Path<String>,
    State(config): State<SiteConfig>,
) -> Result<Html<String>, StatusCode> {
    let path = match match uri.path().strip_prefix('/') {
        Some(s) => s,
        None => "",
    }
    .strip_suffix(page.as_str())
    {
        Some(s) => s,
        None => "",
    };
    let name = format!("pages/{}.templ", path);

    let post = match query_as!(
        Post,
        "SELECT id, name, content, date, tags, owner, status FROM posts WHERE name IS ?",
        name
    )
    .fetch_one(&config.db_pool.unwrap())
    .await
    {
        Ok(post) => post,
        Err(e) => {
            log::error!("{}", e);
            return Err(StatusCode::NOT_FOUND);
        }
    };
    match config.templates.read().unwrap().render(path, &post) {
        Ok(x) => Ok(Html(x)),
        Err(e) => {
            log::error!("{e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_page(
    uri: Uri,
    State(config): State<SiteConfig>,
) -> Result<Html<String>, StatusCode> {
    let path = match uri.path().strip_prefix('/') {
        Some(s) => s,
        None => "",
    };
    let name = format!("pages/{}", path);

    match config
        .templates
        .read()
        .unwrap()
        .render(name.as_str(), &path)
    {
        Ok(x) => Ok(Html(x)),
        Err(e) => {
            log::error!("{e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

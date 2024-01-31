use serde::*;
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::DirEntryExt,
    sync::{Arc, RwLock},
};

use axum::{
    extract::{path, Path, Query, State},
    http::{StatusCode, Uri},
    response::Html,
    routing::{get, post},
    Router,
};
use log::error;
use sqlx::{query, query_as, sqlite::SqlitePoolOptions, SqlitePool};
use tinytemplate_async::{format_unescaped, TinyTemplate};
use tower_http::services::ServeDir;

use crate::{
    auth::{sign_in, sign_up, User},
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

fn setup_templates(routes: &HashMap<String, PagePath>, site_path: String) -> TinyTemplate {
    let mut templates = TinyTemplate::new();
    let admin_template = fs::read_to_string("data/admin.templ.html").unwrap();
    templates
        .add_template("admin".to_string(), admin_template)
        .unwrap();
    for ele in fs::read_dir("admin_panel/").unwrap() {
        match ele {
            Ok(entry) => {
                if !entry.path().is_file() {
                    continue;
                }
                let path = entry.path();
                let path = path.strip_prefix("admin_panel/").unwrap();
                let content = fs::read_to_string(entry.path()).unwrap();
                let path = match path.extension() {
                    Some(ext) => path
                        .to_str()
                        .unwrap()
                        .to_string()
                        .strip_suffix(ext.to_str().unwrap())
                        .unwrap()
                        .to_string(),
                    None => path.to_str().unwrap().to_string(),
                };
                templates.add_template(path, content).unwrap();
            }
            Err(e) => {
                log::error!("{}", e);
                continue;
            }
        }
    }
    for (name, path) in routes {
        log::info!("Found template file {name}");
        let content =
            fs::read_to_string(format!("{site_path}/templates/{}", path.path.clone())).unwrap();
        templates
            .add_template(format!("pages{name}"), content)
            .unwrap();
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

async fn handle_admin_panel(
    page: Query<AdminPageTempl>,
    user: User,
    State(config): State<SiteConfig>,
) -> Result<Html<String>, StatusCode> {
    match fs::read_to_string(format!("admin_panel/{}.html", page.path)) {
        Ok(s) => {
            let mut admin_page_content = page.0.clone();
            admin_page_content.content = Some(s);
            admin_page_content.user = user;
            Ok(Html(
                config
                    .templates
                    .read()
                    .unwrap()
                    .render("admin", &admin_page_content)
                    .unwrap(),
            ))
        }
        Err(e) => {
            error!("{:?}", e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

async fn handle_admin_panel_partial(
    Path(page): Path<String>,
    user: User,
    State(config): State<SiteConfig>,
) -> Result<Html<String>, StatusCode> {
    match config.templates.read().unwrap().render(
        page.clone().as_str(),
        &(AdminPageTempl {
            path: page,
            content: None,
            user,
        }),
    ) {
        Ok(rendered) => Ok(Html(rendered)),
        Err(e) => {
            log::error!("{e}");
            Err(StatusCode::NOT_FOUND)
        }
    }
}

fn setup_routes(config: &SiteConfig) -> Router {
    let router = Router::new()
        .nest(
            "/admin",
            Router::new()
                .nest_service("/static", ServeDir::new("admin_panel/static"))
                .route("/partial/:page", get(handle_admin_panel_partial))
                .route("/", get(handle_admin_panel)),
        )
        .nest(
            "/api",
            Router::new()
                .route("/post", get(get_post).post(create_post).delete(delete_post))
                .route("/sign_in", post(sign_in))
                .route("/sign_up", post(sign_up)),
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

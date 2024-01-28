#![feature(exact_size_is_empty)]

use log::error;

use peroxide::config::PeroxideConfig;

use peroxide::site::init_site;

use std::fs;

#[tokio::main]
async fn main() {
    femme::start();
    let config: PeroxideConfig =
        toml::from_str(fs::read_to_string("Peroxide.toml").unwrap().as_str()).unwrap();
    let mut work_group = tokio::task::JoinSet::new();
    for dir in config.directories.into_iter() {
        work_group.spawn(init_site(dir));
    }
    while let Some(result) = work_group.join_next().await {
        error!("{:?}", result);
    }
}

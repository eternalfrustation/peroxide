#![feature(exact_size_is_empty)]

use log::error;

use clap::Parser;

use peroxide::config::PeroxideConfig;

use peroxide::site::init_site;

use std::fs;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(short, long, default_value_t = String::from("Peroxide.toml"))]
    config: String,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let args = Args::parse();
    let config: PeroxideConfig =
        toml::from_str(fs::read_to_string(args.config).unwrap().as_str()).unwrap();
    let mut work_group = tokio::task::JoinSet::new();
    for dir in config.directories.into_iter() {
        work_group.spawn(init_site(dir));
    }
    while let Some(result) = work_group.join_next().await {
        error!("{:?}", result);
    }
}

//! Read-only HTTP API over ClickHouse (settled history) and Dragonfly (the tip). See docs/api.md.

mod ch;
mod registry;
mod routes;
mod stream;
mod tip;

use std::sync::Arc;

pub struct App {
    pub ch: ch::ClickHouse,
    pub tip: tip::Tip,
    pub relay: stream::Relay,
    pub chains: Vec<u32>,
}

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("load-registry") {
        let ch = ch::ClickHouse::new(env("CLICKHOUSE_URL", "http://127.0.0.1:8123"));
        if let Err(e) = registry::load(&ch).await {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    let chains: Vec<u32> = env("CHAINS", "1,8453,43114,5042")
        .split(',')
        .map(|c| c.trim().parse().expect("CHAINS must be comma-separated chain ids"))
        .collect();
    let redis = redis::Client::open(env("DRAGONFLY_URL", "redis://127.0.0.1:6379")).expect("DRAGONFLY_URL");
    let app = Arc::new(App {
        ch: ch::ClickHouse::new(env("CLICKHOUSE_URL", "http://127.0.0.1:8123")),
        tip: tip::Tip::new(redis.clone()),
        relay: stream::Relay::start(redis, chains.clone()),
        chains,
    });
    let listen = env("API_LISTEN", "127.0.0.1:8080");
    let listener = tokio::net::TcpListener::bind(&listen).await.expect("bind API_LISTEN");
    eprintln!("api listening on {listen}");
    axum::serve(listener, routes::router(app))
        .with_graceful_shutdown(async { tokio::signal::ctrl_c().await.ok(); })
        .await
        .expect("server");
}

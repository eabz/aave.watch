mod calls;
mod fetch;
mod live;
mod model;
mod process;
mod tip;
mod v3;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("fetch") => fetch::run().await,
        Some("process") => process::run().await,
        _ => {
            eprintln!("usage: aave-watch-indexer fetch | process");
            std::process::exit(2);
        }
    }
}

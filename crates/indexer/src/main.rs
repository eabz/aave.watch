mod calls;
mod daily;
mod fetch;
mod hubs;
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
        Some("daily") => daily::run().await,
        Some("backfill-hubs") => hubs::run().await,
        _ => {
            eprintln!("usage: aave-watch-indexer fetch | process | daily [chain_id [from to]] | backfill-hubs [chain_id [upto_block]]");
            std::process::exit(2);
        }
    }
}

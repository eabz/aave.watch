mod fetch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("fetch") => fetch::run().await,
        _ => {
            eprintln!("usage: aave-watch-indexer fetch");
            std::process::exit(2);
        }
    }
}

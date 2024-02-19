pub mod db;
pub mod looper;

use db::{get_postgres_pool, PgClient};
use looper::{ctrl_c_handler, make_looper};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn, Level};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{filter::Targets, layer::SubscriberExt, Registry};

async fn is_batch(batch_code: &str, pg_conn: &PgClient) -> anyhow::Result<bool> {
    let stmt = pg_conn
        .prepare("SELECT batch_code FROM resident_set_update_batch(p_batch_code := $1)")
        .await?;
    let res = pg_conn.query(&stmt, &[&batch_code]).await?;
    Ok(!res.is_empty())
}

fn prepare_log(app_name: &str) {
    let formatting_layer = BunyanFormattingLayer::new(app_name.into(), std::io::stdout);
    let filter = Targets::new().with_target(app_name, Level::INFO);
    let subscriber = Registry::default()
        .with(filter)
        .with(JsonStorageLayer)
        .with(formatting_layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    prepare_log("resident");
    let pg_url =
        std::env::var("PG_URL").unwrap_or("postgres://user:pass@localhost:5432/web".to_owned());
    let pg_pool = get_postgres_pool(&pg_url)?;
    let token: CancellationToken = CancellationToken::new();

    let handles = vec![make_looper(
        pg_pool.clone(),
        token.clone(),
        "*/10 * * * * *",
        |&now: &_, pg_conn: _| async move {
            info!("定期的に処理する何か1 {}", now);
            match is_batch("minutely_batch", &pg_conn).await {
                Ok(res) => {
                    if res {
                        // 本当にやりたいバッチ処理
                        let _result = pg_conn.query("SELECT pg_sleep(5)", &[]).await.unwrap();
                    } else {
                        info!("is_batch is false");
                    }
                }
                Err(e) => {
                    warn!("is_batch error={}", e);
                }
            }
        },
        || async move {
            info!("graceful stop looper 1");
        },
    )];

    #[allow(clippy::let_underscore_future)]
    let _ = ctrl_c_handler(token);
    for handle in handles {
        handle.await.unwrap();
    }
    Ok(())
}

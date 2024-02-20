pub mod db;
pub mod looper;

use db::{get_postgres_pool, PgClient};
use looper::{ctrl_c_handler, make_looper, make_worker};
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

async fn add_task(data_json: &serde_json::Value, pg_conn: &PgClient) -> anyhow::Result<()> {
    let stmt = pg_conn
        .prepare("INSERT INTO public.workers (data_json) VALUES ($1)")
        .await?;
    let _ = pg_conn.execute(&stmt, &[&data_json]).await?;
    Ok(())
}

async fn get_task(pg_conn: &PgClient) -> anyhow::Result<Option<serde_json::Value>> {
    let stmt = pg_conn
        .prepare("SELECT data_json FROM resident_set_delete_worker()")
        .await?;
    let res = pg_conn.query(&stmt, &[]).await?;
    Ok(if res.is_empty() {
        None
    } else {
        let data_json = res[0].get(0);
        Some(data_json)
    })
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

    let handles = vec![
        make_looper(
            pg_pool.clone(),
            token.clone(),
            "*/10 * * * * *",
            60,
            |&now: &_, pg_conn: _| async move {
                info!("定期的に処理する何か1 {}", now);
                match is_batch("minutely_batch", &pg_conn).await {
                    Ok(res) => {
                        if res {
                            // 本当にやりたいバッチ処理
                            add_task(&serde_json::json!({"now": now}), &pg_conn)
                                .await
                                .unwrap();
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
        ),
        make_worker(
            pg_pool.clone(),
            token.clone(),
            60,
            60,
            |&now: &_, _pg_conn: _| async move {
                info!("データがあれば処理する何か1 {}", now);
                match get_task(&_pg_conn).await {
                    Ok(Some(data_json)) => {
                        info!("data_json={}", data_json);
                        0
                    }
                    Ok(None) => {
                        info!("no data");
                        60
                    }
                    Err(e) => {
                        warn!("get_task error={}", e);
                        60
                    }
                }
            },
            || async move {
                info!("graceful stop worker 1");
            },
        ),
    ];

    #[allow(clippy::let_underscore_future)]
    let _ = ctrl_c_handler(token);
    for handle in handles {
        handle.await.unwrap();
    }
    Ok(())
}

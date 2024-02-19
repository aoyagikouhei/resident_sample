pub mod db;
pub mod looper;

use db::{get_postgres_pool, PgClient};
use looper::{ctrl_c_handler, make_looper};
use tokio_util::sync::CancellationToken;

async fn is_batch(batch_code: &str, pg_conn: &PgClient) -> anyhow::Result<bool> {
    let stmt = pg_conn.prepare("SELECT batch_code FROM resident_set_update_batch(p_batch_code := $1)").await?;
    let res = pg_conn.query(&stmt, &[&batch_code]).await?;
    Ok(res.len() > 0)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pg_url = std::env::var("PG_URL").unwrap_or("postgres://user:pass@localhost:5432/web".to_owned());
    let pg_pool = get_postgres_pool(&pg_url)?;
    let token: CancellationToken = CancellationToken::new();

    let handles = vec![make_looper(
        pg_pool.clone(),
        token.clone(),
        "*/10 * * * * *",
        |&now: &_, pg_conn: _| async move {
            println!("定期的に処理する何か1 {}", now);
            match is_batch("minutely_batch", &pg_conn).await {
                Ok(res) => if res {
                    // 本当にやりたいバッチ処理
                    let _result = pg_conn.query("SELECT pg_sleep(5)", &[]).await.unwrap();
                } else {
                    println!("is_batch is false");
                },
                Err(e) => {
                    println!("is_batch error={}", e);
                }
            }
        },
        || async move {
            println!("graceful stop looper 1");
        }
    )];

    #[allow(clippy::let_underscore_future)]
    let _ = ctrl_c_handler(token);
    for handle in handles {
        handle.await.unwrap();
    }
    Ok(())
}

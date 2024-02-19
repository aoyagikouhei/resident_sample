use std::time::Duration;

use deadpool_postgres::tokio_postgres::NoTls;

pub type PgPool = deadpool_postgres::Pool;
pub type PgClient = deadpool_postgres::Client;

pub fn get_postgres_pool(url: &str) -> anyhow::Result<PgPool> {
    let pg_url = url::Url::parse(&url)?;
    let dbname = match pg_url.path_segments() {
        Some(mut res) => res.next(),
        None => Some("web"),
    };
    let pool_config = deadpool_postgres::PoolConfig {
        max_size: 2,
        timeouts: deadpool_postgres::Timeouts { 
            wait: Some(Duration::from_secs(2)),
            ..Default::default()
        },
        ..Default::default()
    };
    let cfg = deadpool_postgres::Config {
        user: Some(pg_url.username().to_string()),
        password: pg_url.password().map(|password| password.to_string()),
        dbname: dbname.map(|dbname| dbname.to_string()),
        host: pg_url.host_str().map(|host| host.to_string()),
        pool: Some(pool_config),
        ..Default::default()
    };
    let res = cfg.create_pool(Some(deadpool_postgres::Runtime::Tokio1), NoTls)?;
    Ok(res)
}

pub async fn get_postgres_client(
    pool: &deadpool_postgres::Pool,
) -> anyhow::Result<PgClient> {
    pool.get().await.map_err(Into::into)
}

use std::{future::Future, str::FromStr, time::Duration};

use chrono::prelude::*;
use cron::Schedule;
use tokio::{signal::ctrl_c, spawn, task::JoinHandle, time::sleep};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::db::{get_postgres_client, PgClient, PgPool};

pub fn ctrl_c_handler(token: CancellationToken) -> JoinHandle<()> {
    spawn(async move {
        ctrl_c().await.unwrap();
        info!("received ctrl-c");
        token.cancel();
    })
}

pub fn make_looper<Fut1, Fut2>(
    pg_pool: PgPool,
    token: CancellationToken,
    expression: &str,
    f: impl Fn(&DateTime<Utc>, PgClient) -> Fut1 + Send + Sync + 'static,
    g: impl Fn() -> Fut2 + Send + Sync + 'static,
) -> JoinHandle<()>
where
    Fut1: Future<Output = ()> + Send,
    Fut2: Future<Output = ()> + Send,
{
    let expression = expression.to_owned();
    spawn(async move {
        let schedule = Schedule::from_str(&expression).unwrap();
        let mut next_tick = schedule.upcoming(Utc).next().unwrap();
        loop {
            // グレースフルストップのチェック
            if token.is_cancelled() {
                g().await;
                break;
            }

            let now = Utc::now();
            if now >= next_tick {
                // 定期的に行う処理実行
                match get_postgres_client(&pg_pool).await {
                    Ok(pg_conn) => {
                        f(&now, pg_conn).await;
                    }
                    Err(e) => {
                        // エラーが出たので、ここでは何もしないで次に期待する
                        warn!("get_postgres_client error={}", e);
                    }
                }

                // 次の時間取得
                next_tick = schedule.upcoming(Utc).next().unwrap();
            }

            // 次の時間計算
            sleep(Duration::from_secs(std::cmp::min(
                (next_tick - now).num_seconds() as u64,
                60,
            )))
            .await;
        }
    })
}

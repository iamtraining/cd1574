use async_trait::async_trait;
// use tracing::debug;

use crate::store::{sqlx_queue::SqlxPool, sqlx_queue::RETRIES};

#[async_trait]
pub trait Progress: Send + Sync + std::fmt::Debug {
    async fn get_finished_jobs_count(&self) -> anyhow::Result<i64>;

    async fn get_total_jobs_count(&self) -> anyhow::Result<i64>;
}

#[async_trait]
impl Progress for SqlxPool {
    async fn get_finished_jobs_count(&self) -> anyhow::Result<i64> {
        let result: i64 = sqlx::query_scalar(
            "select count(*) from public.nomenclatures  
			where is_finished = true 
			or retries = $1",
        )
        .bind(&RETRIES)
        .fetch_one(&self.client)
        .await?;
        Ok(result)
    }

    async fn get_total_jobs_count(&self) -> anyhow::Result<i64> {
        let result: i64 = sqlx::query_scalar("select count(*) from public.nomenclatures ")
            .bind(&RETRIES)
            .fetch_one(&self.client)
            .await?;
        Ok(result)
    }
}

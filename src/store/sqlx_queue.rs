use {
    crate::store::models::Nomenclature,
    async_trait::async_trait,
    serde::{Deserialize, Serialize},
    sqlx::{
        postgres::PgPoolOptions,
        {Pool, Postgres},
    },
    tracing::{debug, info, trace},
};

pub const RETRIES: i64 = 3;

#[async_trait]
pub trait Queue: Send + Sync + std::fmt::Debug {
    /// забирает из таблицы нужное количество джоб
    async fn pull(&self, shard: &str, jobs: i64) -> anyhow::Result<Vec<Nomenclature>>;
    /// будет ставить метку о завершении работы
    async fn finish_job(&self, shard: &str, nm_id: i64) -> anyhow::Result<()>;
    /// вписывает недоступные ссылки картинок и инкрементирует счетчик ретраев
    async fn fail_job(&self, shard: &str, nm_id: i64) -> anyhow::Result<()>;
    /// полностью очищает таблицу
    async fn clear(&self) -> anyhow::Result<()>;
    /// батч для завершенных
    async fn batch_finish_jobs(
        &self,
        shard: &str,
        nms: Vec<(i64, i16, String)>,
    ) -> anyhow::Result<()>;
    /// батч для ошибок
    async fn batch_fail_jobs(&self, shard: &str, nms: Vec<i64>) -> anyhow::Result<()>;
}

#[derive(Debug)]
pub struct SqlxPool {
    pub client: Pool<Postgres>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Database {
    host: String,
    port: String,
    user: String,
    password: String,
    database: String,
}

impl SqlxPool {
    pub async fn new(
        host: String,
        port: String,
        user: String,
        password: String,
        database: String,
    ) -> anyhow::Result<Self> {
        let client = Self::connect(host, port, user, password, database).await?;
        Ok(Self { client })
    }

    async fn connect(
        host: String,
        port: String,
        user: String,
        password: String,
        database: String,
    ) -> anyhow::Result<Pool<Postgres>> {
        info!("connecting to database {:}", host);
        Ok(PgPoolOptions::new()
            .max_connections(250)
            .connect(&build_config(user, password, host, port, database))
            .await?)
    }
}

#[async_trait]
impl Queue for SqlxPool {
    async fn pull(&self, shard: &str, jobs: i64) -> anyhow::Result<Vec<Nomenclature>> {
        let result: Vec<Nomenclature> = sqlx::query_as(&format!(
            "update {}
        set in_process = true
        where nm_id in (
            select nm_id from {}
            where in_process = false and is_finished = false and retries < $1
            for update skip locked
            limit $2
        )
        returning *",
            shard, shard
        ))
        .bind(&RETRIES)
        .bind(&jobs)
        .fetch_all(&self.client)
        .await?;
        Ok(result)
    }

    async fn finish_job(&self, shard: &str, nm_id: i64) -> anyhow::Result<()> {
        sqlx::query(&format!(
            "update {}
        set in_process = false, is_finished = true
        where nm_id = $1",
            shard
        ))
        .bind(&nm_id)
        .execute(&self.client)
        .await?;
        Ok(())
    }

    async fn fail_job(&self, shard: &str, nm_id: i64) -> anyhow::Result<()> {
        sqlx::query(&format!(
            "UPDATE {}
            SET in_process = false, retries = retries + 1
            WHERE nm_id = $1",
            shard
        ))
        .bind(&nm_id)
        .execute(&self.client)
        .await?;
        Ok(())
    }

    async fn clear(&self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM public.nomenclatures")
            .execute(&self.client)
            .await?;
        Ok(())
    }

    async fn batch_finish_jobs(
        &self,
        shard: &str,
        nms: Vec<(i64, i16, String)>,
    ) -> anyhow::Result<()> {
        let query = generate_batch_finish_jobs(shard, nms);
        debug!("batch_finish_jobs>>> {query}");
        sqlx::query(&query).execute(&self.client).await?;
        Ok(())
    }

    async fn batch_fail_jobs(&self, shard: &str, nms: Vec<i64>) -> anyhow::Result<()> {
        let query = generate_batch_fail_jobs(shard, nms);
        // println!("batch_fail_jobs>>> {query}");
        sqlx::query(&query).execute(&self.client).await?;
        Ok(())
    }
}

fn generate_batch_finish_jobs(shard: &str, nms: Vec<(i64, i16, String)>) -> String {
    let mut values = String::from("(values ");
    let mut delimiter = ',';
    for (idx, (nm_id, new_pics_count, good_links)) in nms.iter().enumerate() {
        if idx == nms.len() - 1 {
            delimiter = ' ';
        }
        values
            .push_str(format!("({nm_id}, '{good_links}', {new_pics_count}, false, true)").as_str());
        values.push(delimiter);
    }
    values.push(')');
    let res = format!(
        "update {shard} as n set
    		in_process = c.in_process,
    		is_finished = c.is_finished,
            good_links = c.good_links,
            new_pics_count = c.new_pics_count
		from {values} as c(nm_id, good_links, new_pics_count, in_process, is_finished)
		where c.nm_id = n.nm_id"
    );
    // println!("here {res}");
    res
}

fn generate_batch_fail_jobs(shard: &str, nms: Vec<i64>) -> String {
    let mut values = String::from("(values ");
    let mut delimiter = ',';
    for (idx, nm_id) in nms.iter().enumerate() {
        if idx == nms.len() - 1 {
            delimiter = ' ';
        }
        values.push_str(format!("({nm_id}, false, true)").as_str());
        values.push(delimiter);
    }
    values.push(')');
    format!(
        "update {shard} as n set
			in_process = c.in_process,
			retries = retries + 1
		from {values} as c(nm_id, in_process)
		where c.nm_id = n.nm_id"
    )
}

pub fn build_config(
    user: String,
    password: String,
    host: String,
    port: String,
    dbname: String,
) -> String {
    format!("postgres://{user}:{password}@{host}:{port}/{dbname}?sslmode=disable")
}

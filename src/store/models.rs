#[derive(Debug, sqlx::FromRow)]
pub struct Nomenclature {
    pub nm_id: i64,
    pub old_pics_count: i16,
    pub new_pics_count: Option<i16>,
    pub good_links: String,
    pub in_process: bool,
    pub is_finished: bool,
    pub retries: i64,
}

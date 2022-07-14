use arc_swap::ArcSwap;
use std::sync::atomic;
use std::sync::Arc;

use crate::store::bar::Progress;

pub struct Bar {
    width: usize,
    bar: Box<dyn Progress>,
    name: String,
    output: ArcSwap<String>,
    finished: atomic::AtomicI64,
    total: i64,
}

impl Bar {
    pub fn new<S: Into<String>>(bar: impl Progress + 'static, total: i64, name: S) -> Self {
        let name = name.into();
        Self {
            width: 100,
            bar: Box::new(bar),
            name,
            output: Arc::new(String::new()).into(),
            finished: atomic::AtomicI64::new(0),
            total,
        }
    }

    pub fn get_status(&self) -> Arc<String> {
        self.output.load_full()
    }

    pub async fn process(&self) {
        println!("here");
        loop {
            println!("loop");

            let finished = self.bar.get_finished_jobs_count().await.unwrap();

            self.finished.store(finished, atomic::Ordering::Release);
            let total = self.total;

            println!("finished: {}", self.total);
            println!("total: {}", finished);

            let w = (self.width / 2) - 7;
            let f = (((w as u64) * (finished as u64) / (self.total as u64)) as usize).min(w - 1);
            let e = (w - 1) - f;

            let output = {
                if finished < self.total {
                    format!(
                        "{}\r{:<l$} {:3}{} [{:#>f$}>{:->e$}] {:3}%\nfinished: {finished}, total: {total}",
                        self.total,
                        &self.name,
                        finished,
                        ">",
                        "",
                        "",
                        (100 * (finished as u64)) / (self.total as u64),
                        l = self.width - w - 8 - 5,
                        f = f,
                        e = e,
                    )
                } else {
                    format!(
                        "{:<l$} {:3}{} [{:#>f$}] 100%\n{finished}, total: {total}",
                        &self.name,
                        "1",
                        '>',
                        "",
                        l = self.width - w - 8 - 5,
                        f = f,
                    )
                }
            };
            println!("{}", output);

            self.output.store(Arc::new(output));

            tokio::time::sleep(std::time::Duration::from_secs(150)).await
        }
    }
}

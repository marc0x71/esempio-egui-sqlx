mod app;
mod db;
mod model;
mod ui;

use app::TodoApp;

use crate::{
    db::{
        DbHandle,
        repository::{create_pool, initialize},
    },
    ui::TodoUi,
};

fn main() -> eframe::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Error creating the Tokio runtime");

    let pool = runtime
        .block_on(async {
            let pool = create_pool().await?;
            initialize(&pool).await?;
            Ok::<_, sqlx::Error>(pool)
        })
        .expect("Error initializing the DB");

    eframe::run_native(
        "Todo SQLx",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            let db = DbHandle::new(runtime, pool, cc.egui_ctx.clone());
            let app = TodoApp::new(db);
            Ok(Box::new(TodoUi::new(cc, app)))
        }),
    )
}

mod app;
mod db;
mod model;
mod ui;

use std::{fs, path::PathBuf};

use app::TodoApp;
use directories::ProjectDirs;

use crate::{
    db::{
        DbHandle,
        repository::{create_pool, initialize},
    },
    ui::TodoUi,
};

pub fn database_path() -> std::io::Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "marc0x71", "TodoApp").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Unable to determine application data directory",
        )
    })?;

    fs::create_dir_all(dirs.data_local_dir())?;
    Ok(dirs.data_local_dir().join("app.db"))
}

fn main() -> eframe::Result<()> {
    let db_path = database_path().expect("Unable to determine database path");
    println!("using database {db_path:?}");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Error creating the Tokio runtime");

    let pool = runtime
        .block_on(async {
            let pool = create_pool(&db_path).await?;
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

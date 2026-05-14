pub mod addon_parser;
pub mod db;
pub mod game_data;
pub mod gear_resolver;
pub mod item_db;
pub mod log_buffer;
pub mod models;
pub mod profileset_generator;
pub mod result_parser;
pub mod server;
pub mod simc_runner;
pub mod talent_normalize;
pub mod types;

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;
    use std::sync::Once;

    static LOAD_GAME_DATA: Once = Once::new();

    /// Loads the compacted game-data fixtures used by tests that exercise
    /// item_db lookups (bonuses, gems, enchants). Idempotent across the test
    /// process — call from any test that needs real game data.
    pub(crate) fn ensure_game_data_loaded() {
        LOAD_GAME_DATA.call_once(|| {
            let data_dir =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../resources/data-compacted");
            crate::item_db::load(&data_dir);
        });
    }
}

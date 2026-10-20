/// the integration tests share one database and wipe its tables on setup; every test that
/// touches it holds this lock for its whole duration, so they never see each other's rows
pub static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

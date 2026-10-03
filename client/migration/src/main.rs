use std::env;

use sea_orm::Database;
use sea_orm_migration::MigratorTrait;

use migration::Migrator;
use migration::connection::connect_options;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let _ = dotenvy::dotenv();

    let command = env::args().nth(1).unwrap_or_else(|| "up".to_string());

    let db_url = env::var("APP__DATABASE__URL").expect("APP__DATABASE__URL is not set");
    let db_key = env::var("APP__DATABASE__ENCRYPTION_KEY")
        .expect("APP__DATABASE__ENCRYPTION_KEY is not set");
    let salt = env::var("APP__DATABASE__SALT").expect("APP__DATABASE__SALT is not set");

    let options =
        connect_options(&db_url, &db_key, &salt).expect("failed to build database options");

    let connection = Database::connect(options)
        .await
        .expect("failed to connect to database");

    match command.as_str() {
        "up" => Migrator::up(&connection, None)
            .await
            .expect("migration up failed"),
        "down" => Migrator::down(&connection, Some(1))
            .await
            .expect("migration down failed"),
        "status" => Migrator::status(&connection)
            .await
            .expect("migration status failed"),
        other => panic!("unknown migration command: {other} (expected up|down|status)"),
    }
}

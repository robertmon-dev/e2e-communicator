use std::env;
use std::fmt::Write as _;

use orion::kdf::{Password, Salt, derive_key};
use sea_orm::{ConnectOptions, Database};
use sea_orm_migration::MigratorTrait;
use zeroize::Zeroizing;

use migration::Migrator;

pub const KDF_ITERATIONS: u32 = 3;
pub const KDF_MEMORY_KIB: u32 = 1 << 16;
pub const KDF_KEY_LEN: u32 = 32;

fn derive_sqlcipher_key(pin: &str, salt: &str) -> Zeroizing<String> {
    let constant_salt = Salt::from_slice(salt.as_bytes()).expect("invalid APP__DATABASE__SALT");
    let password =
        Password::from_slice(pin.as_bytes()).expect("invalid APP__DATABASE__ENCRYPTION_KEY");

    let derived_key = derive_key(
        &password,
        &constant_salt,
        KDF_ITERATIONS,
        KDF_MEMORY_KIB,
        KDF_KEY_LEN,
    )
    .expect("sqlcipher key derivation failed");

    let mut hex_key = Zeroizing::new(String::with_capacity((KDF_KEY_LEN * 2) as usize));
    for b in derived_key.unprotected_as_bytes() {
        let _ = write!(&mut *hex_key, "{b:02x}");
    }

    Zeroizing::new(format!("\"x'{}'\"", hex_key.as_str()))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let _ = dotenvy::dotenv();

    let command = env::args().nth(1).unwrap_or_else(|| "up".to_string());

    let db_url = env::var("APP__DATABASE__URL").expect("APP__DATABASE__URL is not set");
    let db_key = env::var("APP__DATABASE__ENCRYPTION_KEY")
        .expect("APP__DATABASE__ENCRYPTION_KEY is not set");
    let salt = env::var("APP__DATABASE__SALT").expect("APP__DATABASE__SALT is not set");

    let sqlcipher_raw_key = derive_sqlcipher_key(&db_key, &salt);

    let mut options = ConnectOptions::new(db_url);
    options
        .sqlx_logging(false)
        .sqlcipher_key(sqlcipher_raw_key.as_str().to_owned());

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

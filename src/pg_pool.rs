use std::{path::PathBuf, time::Duration};

use sqlx::{
    AssertSqlSafe, PgPool,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};

// Keep this in sync with /docker/postgres/init.sql
const PG_EXTENSIONS: &[&str] = &["btree_gin", "pgcrypto", "pg_trgm", "uuid-ossp"];

pub struct TestPgPoolConfig {
    /// This tells the `sqlx::Migrator` where to look for migration files.
    ///
    /// By default, this fetches from `CARGO_MANIFEST_DIR` env, which during runtime, points
    /// to the service's directory.
    pub migrations: PathBuf,
    pub db_name: String,
    pub host: String,
    pub port: u16,
    pub default_database: String,
    pub username: String,
    pub password: String,
}

/// This [TestResource] provides an ephemeral database pool for services' test cases to run against.
/// This achieves that by:
///
/// 1. Connecting to a running Postgres instance.
/// 2. Creating a template database for the service (if it doesn't already exist) and running the
///    service's migrations on it.
/// 3. Creating the ephemeral database based on the service's template database.
/// 4. Creating a database pool that connects to the ephemeral database and passing it to the test.
pub struct TestPgPool {
    /// The configuration for this test database pool.
    config: TestPgPoolConfig,
    /// The 'manager' db pool, which connects as the default superuser at the default database.
    manager: PgPool,
}

impl TestPgPool {
    pub async fn init(config: TestPgPoolConfig) -> Self {
        // Setup manager db pool
        let options = PgConnectOptions::new()
            .host(&config.host)
            .port(config.port)
            .database(&config.default_database)
            .username(&config.username)
            .password(&config.password);
        let pool = PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(1000))
            .connect_with(options)
            .await
            .inspect_err(|error| {
                eprintln!("Failed to connect to postgres instance: {error}");
                eprintln!(
                    "Is your local postgres instance running on {}:{}?",
                    config.host, config.port
                );
            })
            .unwrap();
        Self {
            config,
            manager: pool,
        }
    }

    pub async fn resource(&self) -> PgPool {
        let template_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_database WHERE datname = $1)",
        )
        .bind(&self.config.db_name)
        .fetch_one(&self.manager)
        .await
        .inspect_err(|error| {
            eprintln!(
                "Failed to check existing database for '{}': {error}",
                self.config.db_name
            );
        })
        .unwrap();

        if !template_exists {
            // Initialise template database
            sqlx::query(AssertSqlSafe(format!(
                "CREATE DATABASE {}",
                self.config.db_name
            )))
            .execute(&self.manager)
            .await
            .inspect_err(|error| {
                eprintln!(
                    "Failed to create database '{}': {error}",
                    self.config.db_name
                );
            })
            .unwrap();

            // Create extensions for template database
            let options = (*self.manager.connect_options())
                .clone()
                .database(&self.config.db_name);
            let template_pool = PgPool::connect_with(options)
                .await
                .inspect_err(|error| {
                    eprintln!(
                        "Failed to connect to database '{}': {error}",
                        self.config.db_name
                    );
                })
                .unwrap();
            for extension in PG_EXTENSIONS {
                let extension_sql = format!("CREATE EXTENSION IF NOT EXISTS \"{extension}\"");
                sqlx::query(AssertSqlSafe(extension_sql))
                    .execute(&template_pool)
                    .await
                    .inspect_err(|error| {
                        eprintln!(
                            "Failed to create extension '{extension}' for database '{}': {error}",
                            self.config.db_name
                        );
                    })
                    .unwrap();
            }

            // Run migrations on template database
            let migrator = Migrator::new(self.config.migrations.as_ref())
                .await
                .inspect_err(|error| {
                    eprintln!(
                        "Failed to create migrator for database '{}': {error}",
                        self.config.db_name
                    );
                })
                .unwrap();
            migrator
                .run(&template_pool)
                .await
                .inspect_err(|error| {
                    eprintln!(
                        "Failed to run migrations for database '{}': {error}",
                        self.config.db_name
                    );
                })
                .unwrap();
        }

        // Connect to database
        let options = (*self.manager.connect_options())
            .clone()
            .database(&self.config.db_name);
        PgPool::connect_with(options)
            .await
            .inspect_err(|error| {
                eprintln!(
                    "Failed to connect to database pool '{}': {error}",
                    self.config.db_name
                );
            })
            .unwrap()
    }
}

//! The pools migration keeps what the user chose, and leaves its triggers
//! working on the rebuilt table.

use sqlx::migrate::{Migrate, Migrator};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

const MEMORY_POOLS: i64 = 20_261_101_130_000;

/// `item_id`, `scope_kind`, `scope_key`, `valid_until`, `forgotten`.
type AttributeRow = (String, Option<String>, Option<String>, Option<String>, i64);

async fn before_pools() -> (SqlitePool, Migrator) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrator: Migrator = sqlx::migrate!("./migrations");
    {
        let mut connection = pool.acquire().await.unwrap();
        connection.ensure_migrations_table().await.unwrap();
        for migration in migrator
            .iter()
            .filter(|m| m.version < MEMORY_POOLS && !m.migration_type.is_down_migration())
        {
            connection.apply(migration).await.unwrap();
        }
    }
    (pool, migrator)
}

async fn exec(pool: &SqlitePool, statement: &str) {
    sqlx::query(statement)
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("{statement}: {e}"));
}

async fn seed_item(pool: &SqlitePool, id: &str) {
    exec(pool, &format!("INSERT INTO conversation_memory_items(id,conversation_id,kind,label,created_at_sequence,changed_at_sequence,created_at,updated_at) VALUES ('{id}','c1','user_fact','{id}',1,1,'2026-11-01T00:00:00Z','2026-11-01T00:00:00Z')")).await;
}

#[tokio::test]
async fn sharing_saved_before_pools_is_carried_over() {
    let (pool, migrator) = before_pools().await;
    exec(
        &pool,
        "INSERT INTO conversation_spaces(id,name) VALUES ('s1','One')",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO conversations(id,title,space_id) VALUES ('c1','One','s1')",
    )
    .await;
    for id in ["kept", "spaced", "personal", "dated"] {
        seed_item(&pool, id).await;
    }
    exec(
        &pool,
        "INSERT INTO conversation_memory_attributes(item_id,scope) VALUES ('kept','conversation')",
    )
    .await;
    exec(&pool, "INSERT INTO conversation_memory_attributes(item_id,scope,space_id) VALUES ('spaced','space','s1')").await;
    exec(
        &pool,
        "INSERT INTO conversation_memory_attributes(item_id,scope) VALUES ('personal','personal')",
    )
    .await;
    exec(&pool, "INSERT INTO conversation_memory_attributes(item_id,scope,valid_until,forgotten) VALUES ('dated','conversation','2027-01-01T00:00:00Z',1)").await;

    migrator.run(&pool).await.unwrap();

    let rows: Vec<AttributeRow> = sqlx::query_as(
        "SELECT item_id,scope_kind,scope_key,valid_until,forgotten FROM conversation_memory_attributes ORDER BY item_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        [
            (
                "dated".to_owned(),
                Some("conversation".to_owned()),
                None,
                Some("2027-01-01T00:00:00Z".to_owned()),
                1
            ),
            (
                "kept".to_owned(),
                Some("conversation".to_owned()),
                None,
                None,
                0
            ),
            (
                "personal".to_owned(),
                Some("personal".to_owned()),
                None,
                None,
                0
            ),
            (
                "spaced".to_owned(),
                Some("space".to_owned()),
                Some("s1".to_owned()),
                None,
                0
            ),
        ]
    );
}

#[tokio::test]
async fn a_correction_inherits_the_sharing_of_what_it_replaces() {
    let (pool, migrator) = before_pools().await;
    migrator.run(&pool).await.unwrap();
    exec(
        &pool,
        "INSERT INTO conversation_spaces(id,name) VALUES ('s1','One')",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO conversations(id,title,space_id) VALUES ('c1','One','s1')",
    )
    .await;
    seed_item(&pool, "old").await;
    seed_item(&pool, "new").await;
    exec(&pool, "INSERT INTO conversation_memory_attributes(item_id,scope_kind,scope_key) VALUES ('old','space','s1')").await;

    exec(
        &pool,
        "UPDATE conversation_memory_items SET superseded_by='new' WHERE id='old'",
    )
    .await;

    let scope: (String, String) = sqlx::query_as(
        "SELECT scope_kind,scope_key FROM conversation_memory_attributes WHERE item_id='new'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(scope, ("space".to_owned(), "s1".to_owned()));
}

#[tokio::test]
async fn a_scope_needs_its_key_and_only_a_pool_has_one() {
    let (pool, migrator) = before_pools().await;
    migrator.run(&pool).await.unwrap();
    exec(
        &pool,
        "INSERT INTO conversation_spaces(id,name) VALUES ('s1','One')",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO conversations(id,title,space_id) VALUES ('c1','One','s1')",
    )
    .await;
    seed_item(&pool, "item").await;

    for refused in [
        "INSERT INTO conversation_memory_attributes(item_id,scope_kind) VALUES ('item','space')",
        "INSERT INTO conversation_memory_attributes(item_id,scope_kind,scope_key) VALUES ('item','personal','s1')",
        "INSERT INTO conversation_memory_attributes(item_id,scope_key) VALUES ('item','s1')",
        "INSERT INTO conversation_memory_attributes(item_id,scope_kind) VALUES ('item','lesson')",
    ] {
        assert!(sqlx::query(refused).execute(&pool).await.is_err(), "{refused}");
    }
    exec(
        &pool,
        "INSERT INTO conversation_memory_attributes(item_id) VALUES ('item')",
    )
    .await;
}

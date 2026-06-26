use ducklake_orm::config::{DatabaseConfig, DuckLakeConfig, PoolConfig};
use ducklake_orm::{DuckLakeConnection, DuckLakeError, DuckLakePool, Table};

// ── shared fixture ────────────────────────────────────────────────────────────

#[derive(Table, Debug, PartialEq, Clone)]
#[ducklake(table = "sales", schema = "main")]
pub struct Sale {
    #[ducklake(primary_key)]
    pub id: i64,
    pub amount: f64,
    pub region: String,
}

fn setup() -> DuckLakeConnection {
    let db = DuckLakeConnection::open_in_memory().unwrap();
    db.execute("CREATE TABLE main.sales (id BIGINT PRIMARY KEY, amount DOUBLE, region VARCHAR)")
        .unwrap();
    db
}

fn seed(db: &DuckLakeConnection) {
    for (id, amount, region) in [
        (1, 10.0, "EU"),
        (2, 200.0, "EU"),
        (3, 300.0, "US"),
        (4, 50.0, "US"),
    ] {
        db.insert(Sale {
            id,
            amount,
            region: region.into(),
        })
        .execute()
        .unwrap();
    }
}

// ── SELECT ────────────────────────────────────────────────────────────────────

#[test]
fn insert_and_select_all() {
    let db = setup();
    db.insert(Sale {
        id: 1,
        amount: 99.9,
        region: "EU".into(),
    })
    .execute()
    .unwrap();
    db.insert(Sale {
        id: 2,
        amount: 200.0,
        region: "US".into(),
    })
    .execute()
    .unwrap();

    let rows: Vec<Sale> = db.select::<Sale>().fetch_all().unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn filter_by_region() {
    let db = setup();
    db.insert(Sale {
        id: 1,
        amount: 50.0,
        region: "EU".into(),
    })
    .execute()
    .unwrap();
    db.insert(Sale {
        id: 2,
        amount: 150.0,
        region: "US".into(),
    })
    .execute()
    .unwrap();

    let rows: Vec<Sale> = db
        .select::<Sale>()
        .filter(Sale::region().eq("EU"))
        .fetch_all()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].region, "EU");
}

#[test]
fn filter_gt_amount() {
    let db = setup();
    seed(&db);
    let rows: Vec<Sale> = db
        .select::<Sale>()
        .filter(Sale::amount().gt(100.0))
        .fetch_all()
        .unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn filter_combined_and() {
    let db = setup();
    seed(&db);
    let rows: Vec<Sale> = db
        .select::<Sale>()
        .filter(Sale::amount().gt(100.0).and(Sale::region().eq("EU")))
        .fetch_all()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, 2);
}

#[test]
fn count_with_filter() {
    let db = setup();
    seed(&db);
    let count = db
        .select::<Sale>()
        .filter(Sale::amount().gt(100.0))
        .count()
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn fetch_one_not_found() {
    let db = setup();
    let result = db
        .select::<Sale>()
        .filter(Sale::id().eq(999i64))
        .fetch_one();
    assert!(matches!(result, Err(DuckLakeError::NotFound)));
}

#[test]
fn fetch_optional_some() {
    let db = setup();
    db.insert(Sale {
        id: 42,
        amount: 1.0,
        region: "EU".into(),
    })
    .execute()
    .unwrap();
    let result = db
        .select::<Sale>()
        .filter(Sale::id().eq(42i64))
        .fetch_optional()
        .unwrap();
    assert!(result.is_some());
}

#[test]
fn order_by_desc_and_limit() {
    let db = setup();
    seed(&db);
    let rows: Vec<Sale> = db
        .select::<Sale>()
        .order_by(Sale::amount().desc())
        .limit(2)
        .fetch_all()
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].amount, 300.0);
}

#[test]
fn between_filter() {
    let db = setup();
    seed(&db);
    let rows: Vec<Sale> = db
        .select::<Sale>()
        .filter(Sale::amount().between(20.0, 250.0))
        .fetch_all()
        .unwrap();
    assert_eq!(rows.len(), 2); // 50.0 and 200.0
}

#[test]
fn like_filter() {
    let db = setup();
    db.insert(Sale {
        id: 1,
        amount: 1.0,
        region: "EU-WEST".into(),
    })
    .execute()
    .unwrap();
    db.insert(Sale {
        id: 2,
        amount: 2.0,
        region: "EU-EAST".into(),
    })
    .execute()
    .unwrap();
    db.insert(Sale {
        id: 3,
        amount: 3.0,
        region: "US".into(),
    })
    .execute()
    .unwrap();

    let rows: Vec<Sale> = db
        .select::<Sale>()
        .filter(Sale::region().like("EU%"))
        .fetch_all()
        .unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn bulk_insert() {
    let db = setup();
    let records = vec![
        Sale {
            id: 1,
            amount: 10.0,
            region: "EU".into(),
        },
        Sale {
            id: 2,
            amount: 20.0,
            region: "US".into(),
        },
        Sale {
            id: 3,
            amount: 30.0,
            region: "APAC".into(),
        },
    ];
    let inserted = db.insert_many(records).execute().unwrap();
    assert_eq!(inserted, 3);
    assert_eq!(db.select::<Sale>().count().unwrap(), 3);
}

// ── UPDATE ────────────────────────────────────────────────────────────────────

#[test]
fn update_single_field() {
    let db = setup();
    db.insert(Sale {
        id: 1,
        amount: 10.0,
        region: "EU".into(),
    })
    .execute()
    .unwrap();

    let updated = db
        .update::<Sale>()
        .set(Sale::amount(), 999.0)
        .filter(Sale::id().eq(1i64))
        .execute()
        .unwrap();
    assert_eq!(updated, 1);

    let row = db
        .select::<Sale>()
        .filter(Sale::id().eq(1i64))
        .fetch_one()
        .unwrap();
    assert_eq!(row.amount, 999.0);
}

#[test]
fn update_multiple_fields() {
    let db = setup();
    db.insert(Sale {
        id: 1,
        amount: 10.0,
        region: "EU".into(),
    })
    .execute()
    .unwrap();

    db.update::<Sale>()
        .set(Sale::amount(), 500.0)
        .set(Sale::region(), "APAC")
        .filter(Sale::id().eq(1i64))
        .execute()
        .unwrap();

    let row = db
        .select::<Sale>()
        .filter(Sale::id().eq(1i64))
        .fetch_one()
        .unwrap();
    assert_eq!(row.amount, 500.0);
    assert_eq!(row.region, "APAC");
}

#[test]
fn update_requires_filter() {
    let db = setup();
    let result = db.update::<Sale>().set(Sale::amount(), 0.0).execute();
    assert!(matches!(result, Err(DuckLakeError::Query(_))));
}

#[test]
fn update_requires_set() {
    let db = setup();
    let result = db.update::<Sale>().filter(Sale::id().eq(1i64)).execute();
    assert!(matches!(result, Err(DuckLakeError::Query(_))));
}

#[test]
fn update_all_explicit() {
    let db = setup();
    seed(&db);
    let updated = db
        .update::<Sale>()
        .set(Sale::region(), "GLOBAL")
        .update_all()
        .unwrap();
    assert_eq!(updated, 4);
}

// ── DELETE ────────────────────────────────────────────────────────────────────

#[test]
fn delete_with_filter() {
    let db = setup();
    seed(&db);
    let deleted = db
        .delete::<Sale>()
        .filter(Sale::region().eq("EU"))
        .execute()
        .unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(db.select::<Sale>().count().unwrap(), 2);
}

#[test]
fn delete_requires_filter() {
    let db = setup();
    let result = db.delete::<Sale>().execute();
    assert!(matches!(result, Err(DuckLakeError::Query(_))));
}

#[test]
fn delete_all_explicit() {
    let db = setup();
    seed(&db);
    let deleted = db.delete::<Sale>().delete_all().unwrap();
    assert_eq!(deleted, 4);
    assert_eq!(db.select::<Sale>().count().unwrap(), 0);
}

// ── GROUP BY / HAVING ─────────────────────────────────────────────────────────

#[derive(Table, Debug)]
#[ducklake(table = "sales", schema = "main")]
pub struct SaleByRegion {
    pub region: String,
}

#[test]
fn group_by_region() {
    let db = setup();
    seed(&db);

    let groups: Vec<SaleByRegion> = db
        .select::<SaleByRegion>()
        .group_by("region")
        .order_by(SaleByRegion::region().asc())
        .fetch_all()
        .unwrap();

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].region, "EU");
    assert_eq!(groups[1].region, "US");
}

// ── CONNECTION POOL ───────────────────────────────────────────────────────────

#[test]
fn pool_get_and_query() {
    let pool_cfg = PoolConfig::default();
    let pool = DuckLakePool::open(":memory:", &pool_cfg).unwrap();

    {
        let conn = pool.get().unwrap();
        conn.execute(
            "CREATE TABLE main.sales (id BIGINT PRIMARY KEY, amount DOUBLE, region VARCHAR)",
        )
        .unwrap();
        conn.insert(Sale {
            id: 1,
            amount: 42.0,
            region: "EU".into(),
        })
        .execute()
        .unwrap();
    }
    // DuckDB in-memory is per-connection, so each pool connection is independent.
    // This test validates that the pool can issue multiple checked-out connections.
    let conn2 = pool.get().unwrap();
    // A fresh in-memory connection will have its own empty DB — just check it doesn't panic.
    drop(conn2);
}

#[test]
fn pool_from_config() {
    let cfg = DuckLakeConfig {
        database: DatabaseConfig {
            path: ":memory:".into(),
        },
        pool: PoolConfig::default(),
        ducklake: None,
    };
    let pool = DuckLakePool::from_config(&cfg).unwrap();
    let conn = pool.get().unwrap();
    conn.execute("SELECT 1").unwrap();
}

#[test]
fn pool_update_and_delete() {
    let pool_cfg = PoolConfig::default();
    let pool = DuckLakePool::open(":memory:", &pool_cfg).unwrap();

    let conn = pool.get().unwrap();
    conn.execute("CREATE TABLE main.sales (id BIGINT PRIMARY KEY, amount DOUBLE, region VARCHAR)")
        .unwrap();
    conn.insert(Sale {
        id: 1,
        amount: 10.0,
        region: "EU".into(),
    })
    .execute()
    .unwrap();

    conn.update::<Sale>()
        .set(Sale::amount(), 777.0)
        .filter(Sale::id().eq(1i64))
        .execute()
        .unwrap();

    conn.delete::<Sale>()
        .filter(Sale::id().eq(1i64))
        .execute()
        .unwrap();

    assert_eq!(conn.select::<Sale>().count().unwrap(), 0);
}

// ── IN / NOT IN ───────────────────────────────────────────────────────────────

#[test]
fn in_filter() {
    let db = setup();
    seed(&db);

    let rows: Vec<Sale> = db
        .select::<Sale>()
        .filter(Sale::region().in_(["EU", "US"]))
        .fetch_all()
        .unwrap();
    assert_eq!(rows.len(), 4);
}

#[test]
fn not_in_filter() {
    let db = setup();
    seed(&db);

    let rows: Vec<Sale> = db
        .select::<Sale>()
        .filter(Sale::region().not_in(["EU"]))
        .fetch_all()
        .unwrap();
    assert_eq!(rows.len(), 2); // only US rows
}

#[test]
fn in_empty_returns_no_rows() {
    let db = setup();
    seed(&db);
    let rows: Vec<Sale> = db
        .select::<Sale>()
        .filter(Sale::region().in_(std::iter::empty::<&str>()))
        .fetch_all()
        .unwrap();
    assert_eq!(rows.len(), 0);
}

// ── Aggregates ────────────────────────────────────────────────────────────────

#[test]
fn sum_amount() {
    let db = setup();
    seed(&db);
    let total: Option<f64> = db.select::<Sale>().sum(Sale::amount()).unwrap();
    assert_eq!(total, Some(560.0)); // 10+200+300+50
}

#[test]
fn avg_amount() {
    let db = setup();
    seed(&db);
    let avg: Option<f64> = db.select::<Sale>().avg(Sale::amount()).unwrap();
    assert!((avg.unwrap() - 140.0).abs() < 1e-9);
}

#[test]
fn min_max_amount() {
    let db = setup();
    seed(&db);
    let min: Option<f64> = db.select::<Sale>().min(Sale::amount()).unwrap();
    let max: Option<f64> = db.select::<Sale>().max(Sale::amount()).unwrap();
    assert_eq!(min, Some(10.0));
    assert_eq!(max, Some(300.0));
}

#[test]
fn aggregate_on_empty_table_is_none() {
    let db = setup(); // no seed
    let total: Option<f64> = db.select::<Sale>().sum(Sale::amount()).unwrap();
    assert_eq!(total, None);
}

// ── Upsert ────────────────────────────────────────────────────────────────────

#[test]
fn or_replace_overwrites() {
    let db = setup();
    db.insert(Sale {
        id: 1,
        amount: 10.0,
        region: "EU".into(),
    })
    .execute()
    .unwrap();
    db.insert(Sale {
        id: 1,
        amount: 999.0,
        region: "US".into(),
    })
    .or_replace()
    .execute()
    .unwrap();
    let sale = db
        .select::<Sale>()
        .filter(Sale::id().eq(1i64))
        .fetch_one()
        .unwrap();
    assert_eq!(sale.amount, 999.0);
    assert_eq!(sale.region, "US");
}

#[test]
fn or_ignore_keeps_original() {
    let db = setup();
    db.insert(Sale {
        id: 1,
        amount: 10.0,
        region: "EU".into(),
    })
    .execute()
    .unwrap();
    let rows_affected = db
        .insert(Sale {
            id: 1,
            amount: 999.0,
            region: "US".into(),
        })
        .or_ignore()
        .execute()
        .unwrap();
    assert_eq!(rows_affected, 0);
    let sale = db
        .select::<Sale>()
        .filter(Sale::id().eq(1i64))
        .fetch_one()
        .unwrap();
    assert_eq!(sale.amount, 10.0); // original unchanged
}

// ── RETURNING ─────────────────────────────────────────────────────────────────

#[test]
fn execute_returning_id() {
    let db = setup();
    let id: i64 = db
        .insert(Sale {
            id: 7,
            amount: 50.0,
            region: "EU".into(),
        })
        .execute_returning(Sale::id())
        .unwrap();
    assert_eq!(id, 7);
}

// ── select_raw ────────────────────────────────────────────────────────────────

#[test]
fn select_raw_with_param() {
    use ducklake_orm::query::SqlValue;
    let db = setup();
    seed(&db);
    let rows: Vec<Sale> = db
        .select_raw::<Sale>(
            "SELECT id, amount, region FROM main.sales WHERE region = $1 ORDER BY id",
            &[SqlValue::Text("EU".into())],
        )
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.region == "EU"));
}

// ── migrations feature ────────────────────────────────────────────────────────

#[cfg(feature = "migrations")]
mod migration_tests {
    use ducklake_orm::DuckLakeConnection;
    use ducklake_orm::migration::{Migrator, SqlMigration};

    fn fresh_db() -> DuckLakeConnection {
        DuckLakeConnection::open_in_memory().unwrap()
    }

    #[test]
    fn run_applies_pending_migrations() {
        let db = fresh_db();
        let applied = Migrator::new(&db)
            .add(SqlMigration::new(
                1,
                "create t",
                "CREATE TABLE main.t (id BIGINT)",
                "DROP TABLE main.t",
            ))
            .run()
            .unwrap();
        assert_eq!(applied, 1);
    }

    #[test]
    fn run_is_idempotent() {
        let db = fresh_db();
        let m = || {
            SqlMigration::new(
                1,
                "create t",
                "CREATE TABLE main.t (id BIGINT)",
                "DROP TABLE main.t",
            )
        };
        Migrator::new(&db).add(m()).run().unwrap();
        let second = Migrator::new(&db).add(m()).run().unwrap();
        assert_eq!(second, 0);
    }

    #[test]
    fn rollback_reverses_last_migration() {
        let db = fresh_db();
        let m1 = || {
            SqlMigration::new(
                1,
                "create t",
                "CREATE TABLE main.t (id BIGINT)",
                "DROP TABLE main.t",
            )
        };
        let m2 = || {
            SqlMigration::new(
                2,
                "create u",
                "CREATE TABLE main.u (id BIGINT)",
                "DROP TABLE main.u",
            )
        };

        Migrator::new(&db).add(m1()).add(m2()).run().unwrap();
        let rolled = Migrator::new(&db).add(m1()).add(m2()).rollback(1).unwrap();
        assert_eq!(rolled, 1);

        // v2 should be pending again
        let status = Migrator::new(&db).add(m1()).add(m2()).status().unwrap();
        assert!(status[0].applied);
        assert!(!status[1].applied);
    }

    #[test]
    fn status_reports_applied_and_pending() {
        let db = fresh_db();
        let m1 = || {
            SqlMigration::new(
                1,
                "create t",
                "CREATE TABLE main.t (id BIGINT)",
                "DROP TABLE main.t",
            )
        };
        let m2 = || {
            SqlMigration::new(
                2,
                "create u",
                "CREATE TABLE main.u (id BIGINT)",
                "DROP TABLE main.u",
            )
        };

        Migrator::new(&db).add(m1()).run().unwrap();

        let status = Migrator::new(&db).add(m1()).add(m2()).status().unwrap();
        assert_eq!(status.len(), 2);
        assert!(status[0].applied);
        assert_eq!(status[0].version, 1);
        assert!(!status[1].applied);
        assert_eq!(status[1].version, 2);
    }

    #[test]
    fn duplicate_versions_error() {
        let db = fresh_db();
        let err = Migrator::new(&db)
            .add(SqlMigration::new(1, "a", "SELECT 1", "SELECT 1"))
            .add(SqlMigration::new(1, "b", "SELECT 1", "SELECT 1"))
            .run();
        assert!(err.is_err());
    }
}

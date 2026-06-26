use ducklake_orm::{DuckLakeConnection, DuckLakeError, Table};

#[derive(Table, Debug, PartialEq)]
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

#[test]
fn insert_and_select_all() {
    let db = setup();

    db.insert(Sale { id: 1, amount: 99.9, region: "EU".into() })
        .execute()
        .unwrap();
    db.insert(Sale { id: 2, amount: 200.0, region: "US".into() })
        .execute()
        .unwrap();

    let rows: Vec<Sale> = db.select::<Sale>().fetch_all().unwrap();
    assert_eq!(rows.len(), 2);
}

#[test]
fn filter_by_region() {
    let db = setup();

    db.insert(Sale { id: 1, amount: 50.0, region: "EU".into() })
        .execute()
        .unwrap();
    db.insert(Sale { id: 2, amount: 150.0, region: "US".into() })
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

    for (id, amount) in [(1, 10.0), (2, 200.0), (3, 300.0)] {
        db.insert(Sale { id, amount, region: "EU".into() })
            .execute()
            .unwrap();
    }

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

    db.insert(Sale { id: 1, amount: 200.0, region: "EU".into() })
        .execute()
        .unwrap();
    db.insert(Sale { id: 2, amount: 50.0, region: "EU".into() })
        .execute()
        .unwrap();
    db.insert(Sale { id: 3, amount: 200.0, region: "US".into() })
        .execute()
        .unwrap();

    // amount > 100 AND region = 'EU'
    let rows: Vec<Sale> = db
        .select::<Sale>()
        .filter(Sale::amount().gt(100.0).and(Sale::region().eq("EU")))
        .fetch_all()
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, 1);
}

#[test]
fn count_with_filter() {
    let db = setup();

    for (id, amount) in [(1, 10.0), (2, 200.0), (3, 300.0)] {
        db.insert(Sale { id, amount, region: "EU".into() })
            .execute()
            .unwrap();
    }

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
    db.insert(Sale { id: 42, amount: 1.0, region: "EU".into() })
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

    for (id, amount) in [(1, 100.0), (2, 50.0), (3, 300.0)] {
        db.insert(Sale { id, amount, region: "EU".into() })
            .execute()
            .unwrap();
    }

    let rows: Vec<Sale> = db
        .select::<Sale>()
        .order_by(Sale::amount().desc())
        .limit(2)
        .fetch_all()
        .unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].amount, 300.0);
    assert_eq!(rows[1].amount, 100.0);
}

#[test]
fn bulk_insert() {
    let db = setup();

    let records = vec![
        Sale { id: 1, amount: 10.0, region: "EU".into() },
        Sale { id: 2, amount: 20.0, region: "US".into() },
        Sale { id: 3, amount: 30.0, region: "APAC".into() },
    ];

    let inserted = db.insert_many(records).execute().unwrap();
    assert_eq!(inserted, 3);

    let count = db.select::<Sale>().count().unwrap();
    assert_eq!(count, 3);
}

#[test]
fn between_filter() {
    let db = setup();

    for (id, amount) in [(1, 10.0), (2, 50.0), (3, 150.0), (4, 500.0)] {
        db.insert(Sale { id, amount, region: "EU".into() })
            .execute()
            .unwrap();
    }

    let rows: Vec<Sale> = db
        .select::<Sale>()
        .filter(Sale::amount().between(20.0, 200.0))
        .fetch_all()
        .unwrap();

    assert_eq!(rows.len(), 2);
}

#[test]
fn like_filter() {
    let db = setup();

    db.insert(Sale { id: 1, amount: 1.0, region: "EU-WEST".into() })
        .execute()
        .unwrap();
    db.insert(Sale { id: 2, amount: 2.0, region: "EU-EAST".into() })
        .execute()
        .unwrap();
    db.insert(Sale { id: 3, amount: 3.0, region: "US".into() })
        .execute()
        .unwrap();

    let rows: Vec<Sale> = db
        .select::<Sale>()
        .filter(Sale::region().like("EU%"))
        .fetch_all()
        .unwrap();

    assert_eq!(rows.len(), 2);
}

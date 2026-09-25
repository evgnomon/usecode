use crate::db::{connect, err_text};

pub fn run(dbname: &str) {
    let mut client = connect(Some(dbname));
    let rows = client.query(
        "SELECT schema_name::text, schema_owner::text
         FROM information_schema.schemata ORDER BY schema_name",
        &[],
    );
    match rows {
        Ok(rows) if rows.is_empty() => eprintln!("No schemas found."),
        Ok(rows) => {
            eprintln!("Schemas in {dbname}:");
            for row in rows {
                eprintln!(
                    "  • {} (owner: {})",
                    row.get::<_, String>(0),
                    row.get::<_, String>(1)
                );
            }
        }
        Err(e) => eprintln!("Error: {}", err_text(&e)),
    }
}

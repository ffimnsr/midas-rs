use sqlx::{
  FromRow,
  PgPool,
};

#[derive(Debug, FromRow)]
struct User {
  id: i64,
  name: String,
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
  let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
  let pool = PgPool::connect(&url).await?;

  let count: i64 = sqlx::query_scalar!("select count(*) from users")
    .fetch_one(&pool)
    .await?;
  let users = sqlx::query_as!(User, "select id, name from users order by id")
    .fetch_all(&pool)
    .await?;
  let inserted = sqlx::query!("insert into users (name) values ($1) returning id", "fixture")
    .fetch_one(&pool)
    .await?;

  println!(
    "count={count} users={} inserted={:?}",
    users.len(),
    inserted.id
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  #[test]
  fn query_macros_compile_in_tests() {
    // `cargo sqlx prepare --workspace -- --all-targets` must also record
    // queries inside test modules.
    let _query = sqlx::query!("select count(*) as count from users");
  }
}

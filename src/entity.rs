use crate::models::{Reservation, Session, User};
use base64::Engine;
use chrono::Utc;
use rand::Rng;
use sqlx::PgPool;
use std::error::Error;

type E = Box<dyn Error>;

#[derive(Debug)]
pub struct Entity {
    pool: PgPool,
}

impl Entity {
    pub async fn new(db_url: &str) -> Result<Self, E> {
        let pool = PgPool::connect(db_url).await?;
        Ok(Entity { pool })
    }

    pub async fn user_create(
        &self,
        email: &str,
        password: &str,
        fullname: &str,
        address: &str,
    ) -> Result<(), E> {
        sqlx::query!(
            "INSERT INTO users (email, password, fullname, address) VALUES ($1, $2, $3, $4)",
            email,
            password,
            fullname,
            address
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn user_login(&self, email: &str, password: &str) -> Result<String, E> {
        let user = sqlx::query_as!(
            User,
            "SELECT * FROM users WHERE email = $1 AND password = $2",
            email,
            password
        )
        .fetch_one(&self.pool)
        .await?;

        let mut buf = [0u8; 32];
        rand::rngs::OsRng.fill(&mut buf);
        let token = base64::engine::general_purpose::STANDARD.encode(buf);

        sqlx::query!(
            "INSERT INTO sessions (token, user_id) VALUES ($1, $2)",
            token,
            user.id
        )
        .execute(&self.pool)
        .await?;

        Ok(token)
    }

    pub async fn user_logout(&self, token: &str) -> Result<(), E> {
        sqlx::query!("DELETE FROM sessions WHERE token = $1", token)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn user_get(&self, token: &str) -> Result<User, E> {
        let session = sqlx::query_as!(Session, "SELECT * FROM sessions WHERE token = $1", token)
            .fetch_one(&self.pool)
            .await?;

        let user = sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", session.user_id)
            .fetch_one(&self.pool)
            .await?;

        Ok(user)
    }

    pub async fn reserve_create(
        &self,
        token: &str,
        isbn: &str,
        library_name: &str,
    ) -> Result<(), E> {
        let user = self.user_get(token).await?;

        sqlx::query!(
            "INSERT INTO reservations (user_id, library_id, book_id, status, staging_at) VALUES ($1, $2, $3, $4, $5)",
            user.id,
            library_name,
            isbn,
            "staging",
            Utc::now().naive_utc()
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn reserve_query(
        &self,
        token: &str,
        page_size: u32,
        page: u32,
    ) -> Result<Vec<Reservation>, E> {
        let user = self.user_get(token).await?;

        let reserves = sqlx::query_as!(
            Reservation,
            "SELECT * FROM reservations WHERE user_id = $1 OFFSET $2 LIMIT $3",
            user.id,
            (page_size * page) as i64,
            page_size as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(reserves)
    }

    pub async fn reserve_get(&self, token: &str, id: i64) -> Result<Reservation, E> {
        let user = self.user_get(token).await?;

        let reserve = sqlx::query_as!(
            Reservation,
            "SELECT * FROM reservations WHERE id = $1 AND user_id = $2",
            id,
            user.id,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(reserve)
    }
}

#[cfg(test)]
mod test {
    use super::Entity;
    use std::env;

    #[actix_web::test]
    async fn test_user_create() {
        let appkey = env::var("DATABASE_URL").unwrap();
        let app = Entity::new(&appkey).await.unwrap();
        app.user_create("alice@example2.com", "alice", "アリス", "日本")
            .await
            .unwrap();
    }

    #[actix_web::test]
    async fn test_entity() {
        let appkey = env::var("DATABASE_URL").unwrap();
        let app = Entity::new(&appkey).await.unwrap();

        let token = app.user_login("alice@example2.com", "alice").await.unwrap();
        println!("token: {token:?}");

        let user = app.user_get(&token).await.unwrap();
        println!("user get: {user:?}");

        let reserves = app.reserve_query(&token, 20, 0).await.unwrap();
        println!("reserves query: {reserves:?}");
    }
}

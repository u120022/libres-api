use crate::models;
use actix_web::web::Buf;
use anyhow::Context;
use awc::Client;
use serde_json::Value;
use std::error::Error;

type E = Box<dyn Error>;

pub struct GoogleAppState {
    appkey: String,
}

impl GoogleAppState {
    pub fn new(appkey: &str) -> Self {
        Self {
            appkey: appkey.to_string(),
        }
    }

    pub async fn book_query(
        &self,
        any: &str,
        page_size: u32,
        page: u32,
    ) -> Result<models::BookChunk, E> {
        let start_record = (page_size * page).to_string();
        let max_record = page_size.to_string();

        let reader = Client::default()
            .get("https://www.googleapis.com/books/v1/volumes")
            .query(&[
                ("key", self.appkey.as_str()),
                ("q", any),
                ("startIndex", start_record.as_str()),
                ("maxResults", max_record.as_str()),
            ])?
            .send()
            .await?
            .body()
            .await?
            .reader();

        let root = serde_json::from_reader(reader)?;
        let result = parse_book(root).context("failed to parse")?;

        Ok(result)
    }

    pub async fn book_get(&self, isbn: &str) -> Result<models::Book, E> {
        let any = format!("isbn:{isbn}");

        let reader = Client::default()
            .get("https://www.googleapis.com/books/v1/volumes")
            .query(&[
                ("key", self.appkey.as_str()),
                ("q", any.as_str()),
                ("maxResults", "1"),
            ])?
            .send()
            .await?
            .body()
            .await?
            .reader();

        let root = serde_json::from_reader(reader)?;
        let mut result = parse_book(root).context("failed to parse")?;

        let item = result.items.pop().context("not found")?;

        Ok(item)
    }
}

fn parse_book(node: Value) -> Option<models::BookChunk> {
    let items = node
        .get("items")?
        .as_array()?
        .iter()
        .filter_map(|node| {
            let node = node.get("volumeInfo")?;

            let title = node.get("title")?.as_str()?.to_string();

            let creators = node
                .get("authors")
                .and_then(|node| node.as_array())
                .map(|node| {
                    node.iter()
                        .filter_map(|node| node.as_str())
                        .map(|text| text.to_string())
                        .collect()
                })
                .unwrap_or(vec![]);

            let publishers = node
                .get("publishers")
                .and_then(|node| node.as_str())
                .map(|text| vec![text.to_string()])
                .unwrap_or(vec![]);

            let issued_at = node
                .get("publishedDate")
                .and_then(|node| node.as_str())
                .map(|text| text.to_string());

            let keywords = vec![];

            let descriptions = node
                .get("description")
                .and_then(|node| node.as_str())
                .map(|text| vec![text.to_string()])
                .unwrap_or(vec![]);

            let language = node
                .get("language")
                .and_then(|node| node.as_str())
                .map(|text| text.to_string());

            let isbn = node
                .get("industryIdentifiers")
                .and_then(|node| node.as_array())
                .and_then(|node| {
                    node.iter().find(|node| {
                        node.get("type").and_then(|node| node.as_str()) == Some("ISBN_13")
                    })
                })
                .and_then(|node| node.get("identifier"))
                .and_then(|node| node.as_str())
                .map(|text| text.to_string());

            let annotations = vec![];

            let image_url = node
                .get("imageLinks")
                .and_then(|node| node.get("smallThumbnail"))
                .and_then(|node| node.as_str())
                .map(|node| node.to_string());

            Some(models::Book {
                title,
                descriptions,
                keywords,
                creators,
                publishers,
                issued_at,
                isbn,
                language,
                annotations,
                image_url,
            })
        })
        .collect();

    let total_count = node.get("totalItems")?.as_i64()? as u32;

    Some(models::BookChunk { items, total_count })
}

#[cfg(test)]
mod test {
    use super::GoogleAppState;
    use std::env;

    #[actix_web::test]
    async fn test_google() {
        let appkey = env::var("GOOGLE_APPKEY").unwrap();
        let app = GoogleAppState::new(&appkey);

        let res = app.book_query("ドメイン駆動設計", 20, 0).await.unwrap();
        println!("book query: \"{res:?}\"");
        println!("book query count: \"{:?}\"", res.items.len());

        let res = app.book_get("9784798121963").await.unwrap();
        println!("book get: \"{res:?}\"");
    }
}

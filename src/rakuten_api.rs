use crate::models;
use actix_web::web::Buf;
use anyhow::Context;
use awc::Client;
use serde_json::Value;
use std::error::Error;

type E = Box<dyn Error>;

#[derive(Debug, Default, Clone)]
pub struct RakutenAppState {
    appkey: String,
}

impl RakutenAppState {
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
        let page_size = page_size.to_string();
        let page = (page + 1).to_string();

        let reader = Client::default()
            .get("https://app.rakuten.co.jp/services/api/BooksBook/Search/20170404")
            .query(&[
                ("applicationId", self.appkey.as_str()),
                ("title", any),
                ("hits", page_size.as_str()),
                ("page", page.as_str()),
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
        let reader = Client::default()
            .get("https://app.rakuten.co.jp/services/api/BooksBook/Search/20170404")
            .query(&[
                ("applicationId", self.appkey.as_str()),
                ("isbn", isbn),
                ("hits", "1"),
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
        .get("Items")?
        .as_array()?
        .iter()
        .filter_map(|node| {
            let node = node.get("Item")?;

            let title = node.get("title")?.as_str()?.to_string();

            let creators = node
                .get("author")
                .and_then(|node| node.as_str())
                .map(|text| vec![text.to_string()])
                .unwrap_or(vec![]);

            let publishers = node
                .get("publisherName")
                .and_then(|node| node.as_str())
                .map(|text| vec![text.to_string()])
                .unwrap_or(vec![]);

            let issued_at = node
                .get("salesDate")
                .and_then(|node| node.as_str())
                .map(|text| text.to_string());

            let keywords = vec![];

            let descriptions = node
                .get("itemCaption")
                .and_then(|node| node.as_str())
                .map(|text| vec![text.to_string()])
                .unwrap_or(vec![]);

            let language = None;

            let isbn = node
                .get("isbn")
                .and_then(|node| node.as_str())
                .map(|text| text.to_string());

            let annotations = node
                .get("size")
                .and_then(|node| node.as_str())
                .map(|text| vec![text.to_string()])
                .unwrap_or(vec![]);

            let image_url = node
                .get("smallImageUrl")
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

    let total_count = node.get("count")?.as_i64()? as u32;

    Some(models::BookChunk { items, total_count })
}

#[cfg(test)]
mod test {
    use super::RakutenAppState;
    use std::env;

    #[actix_web::test]
    async fn test_rakuten() {
        let appkey = env::var("RAKUTEN_APPKEY").unwrap();
        let app = RakutenAppState::new(&appkey);

        let res = app.book_query("ドメイン駆動設計", 20, 0).await.unwrap();
        println!("book query: \"{res:?}\"");
        println!("book query count: \"{:?}\"", res.items.len());

        let res = app.book_get("9784798121963").await.unwrap();
        println!("book get: \"{res:?}\"");
    }
}

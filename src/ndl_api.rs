use crate::models;
use actix_web::web::Buf;
use anyhow::Context;
use awc::Client;
use roxmltree::Node;
use std::{error::Error, io::Read};

type E = Box<dyn Error>;

#[derive(Debug, Default)]
pub struct NdlAppState;

impl NdlAppState {
    pub fn new() -> Self {
        Self
    }

    pub async fn book_query(
        &self,
        any: &str,
        page_size: u32,
        page: u32,
    ) -> Result<models::BookChunk, E> {
        let search_query = format!(
            "mediatype=1 AND anywhere=\"{any}\" AND sortBy=\"issued_date/sort.descending\"",
        );
        let max_records = page_size.to_string();
        let start_record = (page * page_size + 1).to_string();

        let mut reader = Client::default()
            .get("https://iss.ndl.go.jp/api/sru")
            .query(&[
                ("operation", "searchRetrieve"),
                ("query", search_query.as_str()),
                ("maximumRecords", max_records.as_str()),
                ("startRecord", start_record.as_str()),
                ("recordPacking", "xml"),
                ("recordSchema", "dcndl_simple"),
            ])?
            .send()
            .await?
            .body()
            .await?
            .reader();

        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        let document = roxmltree::Document::parse(&text)?;
        let root = document.root_element();
        let chunk = parse_book(root).context("failed to parse")?;

        Ok(chunk)
    }

    pub async fn book_get(&self, isbn: &str) -> Result<models::Book, E> {
        let search_query = format!("isbn=\"{isbn}\" AND sortBy=\"issued_date/sort.descending\"");

        let mut reader = Client::default()
            .get("https://iss.ndl.go.jp/api/sru")
            .query(&[
                ("operation", "searchRetrieve"),
                ("query", search_query.as_str()),
                ("maximumRecords", "1"),
                ("recordPacking", "xml"),
                ("recordSchema", "dcndl_simple"),
            ])?
            .send()
            .await?
            .body()
            .await?
            .reader();

        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        let document = roxmltree::Document::parse(&text)?;
        let root = document.root_element();
        let mut chunk = parse_book(root).context("failed to parse")?;

        let item = chunk.items.pop().context("not found")?;

        Ok(item)
    }
}

fn parse_book(node: Node) -> Option<models::BookChunk> {
    const NS_XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

    let items = node
        .children()
        .find(|node| node.has_tag_name("records"))?
        .children()
        .filter(|node| node.has_tag_name("record"))
        .filter_map(|node| {
            let item = node
                .children()
                .find(|node| node.has_tag_name("recordData"))?
                .children()
                .find(|node| node.has_tag_name("dc"))?;

            let title = item
                .children()
                .find(|node| node.has_tag_name("title"))?
                .text()?
                .to_string();

            let descriptions = item
                .children()
                .filter(|node| node.has_tag_name("abstract"))
                .filter_map(|node| node.text())
                .map(|text| text.to_string())
                .collect();

            let keywords = item
                .children()
                .filter(|node| node.has_tag_name("subject"))
                .filter_map(|node| node.text())
                .map(|text| text.to_string())
                .collect();

            let creators = item
                .children()
                .filter(|node| node.has_tag_name("creator"))
                .filter_map(|node| node.text())
                .map(|text| text.to_string())
                .collect();

            let publishers = item
                .children()
                .filter(|node| node.has_tag_name("publisher"))
                .filter_map(|node| node.text())
                .map(|text| text.to_string())
                .collect();

            let issued_at = item
                .children()
                .find(|node| node.has_tag_name("issued"))
                .and_then(|node| node.text())
                .map(|text| text.to_string());

            let isbn = item
                .children()
                .find(|node| {
                    node.has_tag_name("identifier")
                        && node.attribute((NS_XSI, "type")) == Some("dcndl:ISBN")
                })
                .and_then(|node| node.text())
                .map(|text| text.to_string());

            let language = item
                .children()
                .find(|node| node.has_tag_name("language"))
                .and_then(|node| node.text())
                .map(|text| text.to_string());

            let annotations = item
                .children()
                .filter(|node| node.has_tag_name("description"))
                .filter_map(|node| node.text())
                .map(|text| text.to_string())
                .collect();

            let image_url = isbn
                .as_ref()
                .map(|text| format!("https://iss.ndl.go.jp/thumbnail/{text}"));

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

    let total_count = node
        .children()
        .find(|node| node.has_tag_name("numberOfRecords"))?
        .text()?
        .parse()
        .ok()?;

    Some(models::BookChunk { items, total_count })
}

#[cfg(test)]
mod test {
    use super::NdlAppState;

    #[actix_web::test]
    async fn test_ndl() {
        let app = NdlAppState::new();

        let res = app.book_query("ドメイン駆動設計", 20, 0).await.unwrap();
        println!("book query: \"{res:?}\"");
        println!("book query count: \"{:?}\"", res.items.len());

        let res = app.book_get("9784798121963").await.unwrap();
        println!("book get: \"{res:?}\"");
    }
}

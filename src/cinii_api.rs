use crate::models;
use actix_web::web::Buf;
use anyhow::Context;
use awc::Client;
use roxmltree::Node;
use std::{error::Error, io::Read};

type E = Box<dyn Error>;

#[derive(Debug, Default, Clone)]
pub struct CiniiAppState {
    appkey: String,
}

impl CiniiAppState {
    pub fn new(appkey: &str) -> Self {
        Self {
            appkey: appkey.to_string(),
        }
    }

    pub async fn holder_query(
        &self,
        isbn: &str,
        page_size: u32,
        page: u32,
    ) -> Result<models::HolderChunk, E> {
        let mut reader = Client::default()
            .get("https://ci.nii.ac.jp/books/opensearch/search")
            .query(&[("appid", self.appkey.as_str()), ("isbn", isbn)])?
            .send()
            .await?
            .body()
            .await?
            .reader();

        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        let document = roxmltree::Document::parse(&text)?;
        let root = document.root_element();
        let ncid = parse_ncid(root).context("failed to parse")?;

        let mut reader = Client::default()
            .get("https://ci.nii.ac.jp/books/opensearch/holder")
            .query(&[("appid", self.appkey.as_str()), ("ncid", ncid.as_str())])?
            .send()
            .await?
            .body()
            .await?
            .reader();

        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        let document = roxmltree::Document::parse(&text)?;
        let root = document.root_element();
        let chunk = parse_holder(root).context("failed to parse")?;

        let items: Vec<_> = chunk
            .items
            .into_iter()
            .map(|item| models::Holder {
                isbn: isbn.to_string(),
                library_name: item.library_name,
                state: item.state,
            })
            .skip((page_size * page) as usize)
            .take(page_size as usize)
            .collect();

        Ok(models::HolderChunk {
            items,
            total_count: chunk.total_count,
        })
    }
}

fn parse_ncid(node: Node) -> Option<String> {
    let ncid = node
        .children()
        .find(|node| node.has_tag_name("entry"))?
        .children()
        .find(|node| node.has_tag_name("id"))?
        .text()?
        .split('/')
        .last()?
        .to_string();
    Some(ncid)
}

#[derive(Debug, Default, Clone)]
struct HolderChunk {
    items: Vec<Holder>,
    total_count: u32,
}

#[derive(Debug, Default, Clone)]
struct Holder {
    #[allow(dead_code)]
    library_name: String,
    state: models::HolderState,
}

fn parse_holder(node: Node) -> Option<HolderChunk> {
    let items = node
        .children()
        .filter(|node| node.has_tag_name("entry"))
        .filter_map(|node| {
            let library_name = node
                .children()
                .find(|node| node.has_tag_name("title"))?
                .text()?
                .replace(' ', "");

            Some(Holder {
                library_name,
                state: models::HolderState::Exists,
            })
        })
        .collect();

    let total_count = node
        .children()
        .find(|node| node.has_tag_name("totalResults"))?
        .text()?
        .parse()
        .ok()?;

    Some(HolderChunk { items, total_count })
}

#[cfg(test)]
mod test {
    use super::CiniiAppState;
    use std::env;

    #[actix_web::test]
    async fn test_cinii() {
        let appkey = env::var("CINII_APPKEY").unwrap();
        let app = CiniiAppState::new(&appkey);

        let res = app.holder_query("9784001141276", 20, 0).await.unwrap();
        println!("holder query: \"{res:?}\"");
    }
}

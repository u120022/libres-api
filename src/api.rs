use crate::models;
use actix_web::web::Buf;
use anyhow::Context;
use awc::Client;
use geoutils::Location;
use roxmltree::Node;
use std::{
    io::Read,
    sync::{Arc, RwLock},
};

#[derive(Debug, Default, Clone)]
pub struct AppState {
    library_chunk: Arc<RwLock<LibraryChunk>>,
    api_key: String,
}

impl AppState {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            ..Self::default()
        }
    }

    // get and store library all data from external web api
    pub async fn pull_data(&mut self) {
        let mut reader = Client::default()
            .get("https://api.calil.jp/library")
            .query(&[("appkey", self.api_key.as_str())])
            .unwrap()
            .send()
            .await
            .unwrap()
            .body()
            .limit(1024 * 1024 * 16) // 16Mib
            .await
            .unwrap()
            .reader();

        let mut buf = String::new();
        reader.read_to_string(&mut buf).unwrap();

        let document = roxmltree::Document::parse(&buf).unwrap();
        let root = document.root_element();

        let mut library_chunk = self.library_chunk.write().unwrap();
        *library_chunk = library_pull_parse(root).unwrap();
    }

    // search library by pref. and city
    pub async fn library_query(
        &self,
        prefecture: &str,
        city: &str,
        page_size: u32,
        page: u32,
    ) -> models::LibraryChunk {
        let library_chunk = self.library_chunk.read().unwrap();

        let filtered = library_chunk
            .items
            .iter()
            .filter(|item| item.prefecture == *prefecture && item.city == *city);

        let items: Vec<models::Library> = filtered
            .clone()
            .take(page_size as usize)
            .skip((page_size * page) as usize)
            .cloned()
            .map(Library::into)
            .collect();

        let total_count = filtered.count() as u32;

        models::LibraryChunk { items, total_count }
    }

    // search library by geocode
    pub async fn library_geocode_query(
        &self,
        geocode: (f64, f64),
        limit: u32,
    ) -> models::LibraryChunk {
        let library_chunk = self.library_chunk.read().unwrap();

        let current = Location::new(geocode.0, geocode.1);

        let mut ref_items: Vec<_> = library_chunk.items.iter().collect();

        ref_items.sort_by_key(|item| {
            Location::new(item.geocode.0, item.geocode.1)
                .haversine_distance_to(&current)
                .meters() as u32
        });

        let items: Vec<models::Library> = ref_items
            .into_iter()
            .take(limit as usize)
            .cloned()
            .map(Library::into)
            .collect();

        let total_count = items.len() as u32;

        models::LibraryChunk { items, total_count }
    }

    // get library by name
    pub async fn library_get(&self, name: String) -> models::Library {
        let library_chunk = self.library_chunk.read().unwrap();

        let library: models::Library = library_chunk
            .items
            .iter()
            .find(|item| item.name == *name)
            .unwrap()
            .clone()
            .into();

        library
    }

    // get holder state by isbn and library name from external web api
    // relate library name and system id by library all ata
    pub async fn holder_query(&self, isbn: &str, library_names: &[&str]) -> models::HolderChunk {
        let library_chunk = self.library_chunk.read().unwrap();

        let system_ids: Vec<_> = library_names
            .iter()
            .filter_map(|name| library_chunk.items.iter().find(|item| item.name == *name))
            .map(|item| &item.system_id)
            .collect();

        let system_ids: Vec<_> = system_ids
            .iter()
            .map(|system_id| system_id.as_str())
            .collect();

        let chunk = self.holder_query_by_system_ids(&isbn, &system_ids).await;

        let items: Vec<models::Holder> = library_names
            .iter()
            .filter_map(|name| {
                let item = library_chunk.items.iter().find(|item| item.name == *name)?;
                let system_id = &item.system_id;
                let ingroup_id = &item.ingroup_id;

                let state = chunk
                    .items
                    .iter()
                    .find(|item| &item.system_id == system_id && &item.ingroup_id == ingroup_id)
                    .map(|item| &item.state)
                    .cloned()
                    .unwrap_or_default();

                Some(models::Holder {
                    isbn: isbn.to_string(),
                    library_name: name.to_string(),
                    state,
                })
            })
            .collect();

        let total_count = items.len() as u32;

        models::HolderChunk { items, total_count }
    }

    async fn holder_query_by_system_ids(&self, isbn: &str, system_ids: &[&str]) -> HolderChunk {
        let system_id = system_ids.join(",");

        let send_query: Vec<(&str, &str)> = vec![
            ("appkey", &self.api_key),
            ("isbn", isbn),
            ("systemid", &system_id),
            ("format", "xml"),
        ];

        let mut reader = Client::default()
            .get("https://api.calil.jp/check")
            .query(&send_query)
            .unwrap()
            .send()
            .await
            .unwrap()
            .body()
            .await
            .unwrap()
            .reader();

        let mut buf = String::new();
        reader.read_to_string(&mut buf).unwrap();

        let document = roxmltree::Document::parse(&buf).unwrap();
        let root = document.root_element();

        let mut result = holder_get_parse(root)
            .context("invalid format response")
            .unwrap();

        // polling
        while result.has_next {
            std::thread::sleep(std::time::Duration::from_secs(2));

            let send_query: Vec<(&str, &str)> = vec![
                ("appkey", &self.api_key),
                ("session", &result.session),
                ("format", "xml"),
            ];

            let mut reader = Client::default()
                .get("https://api.calil.jp/check")
                .query(&send_query)
                .unwrap()
                .send()
                .await
                .unwrap()
                .body()
                .await
                .unwrap()
                .reader();

            let mut buf = String::new();
            reader.read_to_string(&mut buf).unwrap();

            let document = roxmltree::Document::parse(&buf).unwrap();
            let root = document.root_element();

            result = holder_get_parse(root)
                .context("invalid format response")
                .unwrap();
        }

        result
    }
}

// get library all data impl.
// tempolary library data structure

#[derive(Debug, Default, Clone)]
struct LibraryChunk {
    items: Vec<Library>,
}

impl From<LibraryChunk> for models::LibraryChunk {
    fn from(val: LibraryChunk) -> Self {
        let items: Vec<_> = val.items.into_iter().map(Library::into).collect();
        let total_count = items.len() as u32;
        models::LibraryChunk { items, total_count }
    }
}

#[derive(Debug, Default, Clone)]
struct Library {
    name: String,
    system_id: String,
    ingroup_id: String,
    url: String,
    address: String,
    prefecture: String,
    city: String,
    postcode: String,
    tel: String,
    geocode: (f64, f64),
}

impl From<Library> for models::Library {
    fn from(val: Library) -> Self {
        models::Library {
            name: val.name,
            address: Some(val.address),
            prefecture: Some(val.prefecture),
            city: Some(val.city),
            postcode: Some(val.postcode),
            tel: Some(val.tel),
            url: Some(val.url),
            geocode: Some(val.geocode),
        }
    }
}

fn library_pull_parse(node: Node) -> Option<LibraryChunk> {
    let items: Vec<_> = node
        .children()
        .filter(|node| node.has_tag_name("Library"))
        .filter_map(|node| {
            let name = node
                .children()
                .find(|node| node.has_tag_name("formal"))?
                .text()?
                .to_string();

            let system_id = node
                .children()
                .find(|node| node.has_tag_name("systemid"))?
                .text()?
                .to_string();

            let ingroup_id = node
                .children()
                .find(|node| node.has_tag_name("libkey"))?
                .text()?
                .to_string();

            let url = node
                .children()
                .find(|node| node.has_tag_name("url_pc"))?
                .text()?
                .to_string();

            let address = node
                .children()
                .find(|node| node.has_tag_name("address"))?
                .text()?
                .to_string();

            let prefecture = node
                .children()
                .find(|node| node.has_tag_name("pref"))?
                .text()?
                .to_string();

            let city = node
                .children()
                .find(|node| node.has_tag_name("city"))?
                .text()?
                .to_string();

            let postcode = node
                .children()
                .find(|node| node.has_tag_name("post"))?
                .text()?
                .to_string();

            let tel = node
                .children()
                .find(|node| node.has_tag_name("tel"))?
                .text()?
                .to_string();

            let (lng, lat) = node
                .children()
                .find(|node| node.has_tag_name("geocode"))?
                .text()?
                .split_once(',')?;
            let geocode = (lat.parse().ok()?, lng.parse().ok()?);

            Some(Library {
                name,
                system_id,
                ingroup_id,
                address,
                prefecture,
                city,
                postcode,
                tel,
                url,
                geocode,
            })
        })
        .collect();

    Some(LibraryChunk { items })
}

// search holder state by isbn and system id
// tempolary holder state data structure

#[derive(Debug, Default, Clone)]
struct HolderChunk {
    session: String,
    has_next: bool,
    items: Vec<Holder>,
}

#[derive(Debug, Default, Clone)]
struct Holder {
    system_id: String,
    ingroup_id: String,
    state: models::HolderState,
}

fn holder_get_parse(node: Node) -> Option<HolderChunk> {
    let session = node
        .children()
        .find(|node| node.has_tag_name("session"))?
        .text()?
        .to_string();

    let has_next = node
        .children()
        .find(|node| node.has_tag_name("continue"))?
        .text()?
        != "0";

    let items = node
        .children()
        .find(|node| node.has_tag_name("books"))?
        .children()
        .find(|node| node.has_tag_name("book"))?
        .children()
        .filter(|node| node.has_tag_name("system"))
        .filter_map(|node| {
            let system_id = node.attribute("systemid")?;

            let items = node
                .children()
                .find(|node| node.has_tag_name("libkeys"))?
                .children()
                .filter(|node| node.has_tag_name("libkey"))
                .filter_map(|node| {
                    let ingroup_id = node.attribute("name")?;

                    let state = match node.text()? {
                        "貸出可" | "蔵書あり" => models::HolderState::Exists,
                        "予約中" => models::HolderState::Reserved,
                        "貸出中" => models::HolderState::Borrowed,
                        "館内のみ" => models::HolderState::Inplace,
                        _ => models::HolderState::Nothing,
                    };

                    Some(Holder {
                        system_id: system_id.to_string(),
                        ingroup_id: ingroup_id.to_string(),
                        state,
                    })
                });
            Some(items)
        })
        .flatten()
        .collect();

    Some(HolderChunk {
        session,
        has_next,
        items,
    })
}

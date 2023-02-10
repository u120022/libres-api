use crate::models;
use actix_web::web::Buf;
use anyhow::Context;
use awc::Client;
use geoutils::Location;
use roxmltree::Node;
use std::{
    borrow::Cow,
    error::Error,
    io::Read,
    sync::{Arc, RwLock},
};

type E = Box<dyn Error>;

#[derive(Debug, Default, Clone)]
pub struct CalilAppState {
    library_chunk: Arc<RwLock<LibraryChunk>>,
    api_key: String,
}

impl CalilAppState {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            ..Self::default()
        }
    }

    // get and store library all data from external web api
    pub async fn pull_data(&mut self) -> Result<(), E> {
        let mut reader = Client::default()
            .get("https://api.calil.jp/library")
            .query(&[("appkey", self.api_key.as_str())])?
            .send()
            .await?
            .body()
            .limit(1024 * 1024 * 16) // 16Mib
            .await?
            .reader();

        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;

        let document = roxmltree::Document::parse(&buf)?;
        let root = document.root_element();

        let mut library_chunk = self.library_chunk.write().ok().context("poisoned")?;
        *library_chunk = library_pull_parse(root).context("failed to parse")?;
        Ok(())
    }

    // search library by pref. and city
    pub async fn library_query(
        &self,
        prefecture: &str,
        city: &str,
        page_size: u32,
        page: u32,
    ) -> Result<models::LibraryChunk, E> {
        let library_chunk = self.library_chunk.read().ok().context("poisoned")?;

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

        Ok(models::LibraryChunk { items, total_count })
    }

    // search library by geocode
    pub async fn library_geocode_query(
        &self,
        geocode: (f64, f64),
        limit: u32,
    ) -> Result<models::LibraryChunk, E> {
        let library_chunk = self.library_chunk.read().ok().context("poisoned")?;

        let current = Location::new(geocode.0, geocode.1);

        let mut items: Vec<_> = library_chunk.items.iter().collect();

        items.sort_by_key(|item| {
            Location::new(item.geocode.0, item.geocode.1)
                .haversine_distance_to(&current)
                .meters() as u32
        });

        let items: Vec<models::Library> = items
            .into_iter()
            .take(limit as usize)
            .cloned()
            .map(Library::into)
            .collect();

        let total_count = items.len() as u32;

        Ok(models::LibraryChunk { items, total_count })
    }

    // get library by name
    pub async fn library_get(&self, library_name: &str) -> Result<models::Library, E> {
        let library_chunk = self.library_chunk.read().ok().context("poisoned")?;

        let library: models::Library = library_chunk
            .items
            .iter()
            .find(|item| item.library_name == *library_name)
            .context("not found")?
            .clone()
            .into();

        Ok(library)
    }

    // get holder state by isbn and library name from external web api
    // relate library name and system id by library all ata
    pub async fn holder_query(
        &self,
        isbn: &str,
        library_names: &[&str],
    ) -> Result<models::HolderChunk, E> {
        let library_chunk = self.library_chunk.read().ok().context("poisoned")?;

        let library_chunk: Vec<_> = library_names
            .iter()
            .filter_map(|library_name| {
                library_chunk
                    .items
                    .iter()
                    .find(|item| item.library_name == *library_name)
            })
            .collect();

        let system_ids: Vec<_> = library_chunk
            .iter()
            .map(|item| item.system_id.as_str())
            .collect();

        let mut send_query: Vec<(_, Cow<str>)> = vec![
            ("appkey", Cow::Borrowed(&self.api_key)),
            ("isbn", Cow::Borrowed(isbn)),
            ("systemid", Cow::Owned(system_ids.join(","))),
            ("format", Cow::Borrowed("xml")),
        ];

        let chunk = loop {
            let mut reader = Client::default()
                .get("https://api.calil.jp/check")
                .query(&send_query)?
                .send()
                .await?
                .body()
                .await?
                .reader();

            let mut buf = String::new();
            reader.read_to_string(&mut buf)?;

            let document = roxmltree::Document::parse(&buf)?;
            let root = document.root_element();

            let chunk = holder_get_parse(root).context("failed to parse")?;
            send_query = vec![
                ("appkey", Cow::Borrowed(&self.api_key)),
                ("session", Cow::Owned(chunk.session.clone())),
                ("format", Cow::Borrowed("xml")),
            ];

            if !chunk.has_next {
                break chunk;
            }

            std::thread::sleep(std::time::Duration::from_secs(2));
        };

        let items: Vec<_> = library_chunk
            .iter()
            .map(|item| {
                let library_name = &item.library_name;
                let system_id = &item.system_id;
                let ingroup_id = &item.ingroup_id;

                let state = chunk
                    .items
                    .iter()
                    .find(|item| &item.system_id == system_id && &item.ingroup_id == ingroup_id)
                    .map_or(models::HolderState::Nothing, |item| item.state.clone());

                models::Holder {
                    isbn: isbn.to_string(),
                    library_name: library_name.to_string(),
                    state,
                }
            })
            .collect();

        let total_count = items.len() as u32;

        Ok(models::HolderChunk { items, total_count })
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
    library_name: String,
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
            name: val.library_name,
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
                library_name: name,
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

#[cfg(test)]
mod test {
    use super::CalilAppState;
    use std::env;

    #[actix_web::test]
    async fn test_all() {
        let api_key = env::var("CALIL_APPKEY").unwrap();
        let mut state = CalilAppState::new(&api_key);
        state.pull_data().await.unwrap();

        let res = state
            .library_query("富山県", "射水市", 20, 0)
            .await
            .unwrap();
        println!("query: \"{:?}\"", res);

        let res = state
            .library_geocode_query((36.7077262, 137.0958753), 20)
            .await
            .unwrap();
        println!("geocode query: \"{:?}\"", res);

        let res = state
            .library_get("富山県立大学附属図書館射水館")
            .await
            .unwrap();
        println!("get: \"{:?}\"", res);

        let res = state
            .holder_query("9784001141276", &["富山県立大学附属図書館射水館"])
            .await
            .unwrap();
        println!("holder: \"{:?}\"", res);
    }
}

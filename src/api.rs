use crate::{models, CALIL_APPKEY};
use actix_web::{
    get, post,
    web::{Buf, Data, Json, Path, Query},
    HttpResponse,
};
use anyhow::Context;
use awc::Client;
use geoutils::Location;
use roxmltree::Node;
use serde::Deserialize;
use std::{
    error::Error,
    io::Read,
    sync::{Arc, RwLock},
};

type E = Box<dyn Error>;

// library all data store

pub type LibraryState = Data<Arc<RwLock<LibraryChunk>>>;

pub fn new_state() -> LibraryState {
    Data::new(Arc::new(RwLock::new(LibraryChunk::default())))
}

// get and store library all data from external web api

#[get("/pull_library")]
async fn pull_library(state: LibraryState) -> HttpResponse {
    let Ok(chunk) = library_pull_internal().await else {
        return HttpResponse::InternalServerError().body("failed to pull library data");
    };

    let Ok(mut state) = state.write() else {
        return HttpResponse::InternalServerError().body("poisoned");
    };

    *state = chunk;

    HttpResponse::Ok().body("successful")
}

// search library by pref. and city

#[derive(Deserialize)]
struct LibraryQuery {
    prefecture: String,
    city: String,
    page_size: u32,
    page: u32,
}

#[get("/library")]
async fn library_query(query: Query<LibraryQuery>, state: LibraryState) -> HttpResponse {
    let Ok(state) = state.read() else {
        return HttpResponse::InternalServerError().body("poisoned");
    };

    let iter = state
        .items
        .iter()
        .filter(|item| item.prefecture == *query.prefecture && item.city == *query.city);

    let items: Vec<models::Library> = iter
        .clone()
        .take(query.page_size as usize)
        .skip((query.page_size * query.page) as usize)
        .cloned()
        .map(Library::into)
        .collect();

    let total_count = iter.count() as u32;

    let chunk = models::LibraryChunk { items, total_count };
    HttpResponse::Ok().json(chunk)
}

// search library by geocode

#[derive(Deserialize)]
struct LibraryGeocodeQuery {
    lat: f64,
    lng: f64,
    limit: u32,
}

#[get("/library_geocode")]
async fn library_geocode_query(
    query: Query<LibraryGeocodeQuery>,
    state: LibraryState,
) -> HttpResponse {
    let Ok(state) = state.read() else {
        return HttpResponse::InternalServerError().body("poisoned");
    };

    let current = Location::new(query.lat, query.lng);

    let mut items: Vec<_> = state.items.iter().collect();

    items.sort_by_key(|item| {
        Location::new(item.geocode.0, item.geocode.1)
            .haversine_distance_to(&current)
            .meters() as u32
    });

    let items: Vec<models::Library> = items
        .into_iter()
        .take(query.limit as usize)
        .cloned()
        .map(Library::into)
        .collect();

    let total_count = items.len() as u32;

    let chunk = models::LibraryChunk { items, total_count };
    HttpResponse::Ok().json(chunk)
}

#[get("/library/{_}")]
async fn library_get(name: Path<String>, state: LibraryState) -> HttpResponse {
    let Ok(state) = state.read() else {
        return HttpResponse::InternalServerError().body("poisoned");
    };

    let library = state.items.iter().find(|item| item.name == *name);

    let Some(library) = library else {
        return HttpResponse::NotFound().body("not found");
    };

    let library: models::Library = library.clone().into();
    HttpResponse::Ok().json(library)
}

// get library by name

#[derive(Debug, Default, Clone)]
pub struct LibraryChunk {
    items: Vec<Library>,
}

impl From<LibraryChunk> for models::LibraryChunk {
    fn from(val: LibraryChunk) -> Self {
        let items: Vec<_> = val.items.into_iter().map(Library::into).collect();
        let total_count = items.len() as u32;
        models::LibraryChunk { items, total_count }
    }
}

// tempolary library data structure

#[derive(Debug, Default, Clone)]
pub struct Library {
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

// get library all data impl.

async fn library_pull_internal() -> Result<LibraryChunk, E> {
    let mut reader = Client::default()
        .get("https://api.calil.jp/library")
        .query(&[("appkey", CALIL_APPKEY.as_str())])?
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

    let result = library_pull_parse(root).context("invalid format response")?;
    Ok(result)
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

// get holder state by isbn and library name from external web api
// relate library name and system id by library all ata

#[derive(Deserialize)]
struct HolderQuery {
    isbn: String,
    library_names: Vec<String>,
}

#[post("/holder")]
async fn holder_query(query: Json<HolderQuery>, state: LibraryState) -> HttpResponse {
    let Ok(state) = state.read() else {
        return HttpResponse::InternalServerError().body("poisoned");
    };

    let system_ids: Vec<_> = query
        .library_names
        .iter()
        .filter_map(|name| state.items.iter().find(|item| item.name == *name))
        .map(|item| &item.system_id)
        .collect();

    let system_ids: Vec<_> = system_ids
        .iter()
        .map(|system_id| system_id.as_str())
        .collect();

    let Ok(chunk) = holder_query_internal(&query.isbn, &system_ids).await else {
        return HttpResponse::InternalServerError().body("failed to process");
    };

    let items: Vec<models::Holder> = query
        .library_names
        .iter()
        .filter_map(|name| {
            let item = state.items.iter().find(|item| item.name == *name)?;
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
                isbn: query.isbn.to_string(),
                library_name: name.to_string(),
                state,
            })
        })
        .collect();

    let total_count = items.len() as u32;

    let chunk = models::HolderChunk { items, total_count };
    HttpResponse::Ok().json(chunk)
}

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

// search holder state by isbn and system id

async fn holder_query_internal(isbn: &str, system_ids: &[&str]) -> Result<HolderChunk, E> {
    let system_id = system_ids.join(",");

    let send_query: Vec<(&str, &str)> = vec![
        ("appkey", &CALIL_APPKEY),
        ("isbn", isbn),
        ("systemid", &system_id),
        ("format", "xml"),
    ];

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

    let mut result = holder_get_parse(root).context("invalid format response")?;

    // polling
    while result.has_next {
        std::thread::sleep(std::time::Duration::from_secs(2));

        let send_query: Vec<(&str, &str)> = vec![
            ("appkey", &CALIL_APPKEY),
            ("session", &result.session),
            ("format", "xml"),
        ];

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

        result = holder_get_parse(root).context("invalid format response")?;
    }

    Ok(result)
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

// falback endpoint

pub async fn fallback() -> HttpResponse {
    HttpResponse::NotFound().body("no endpoint, but connection to api is successful.")
}

// test

#[cfg(test)]
mod test {
    use super::{holder_query_internal, library_pull_internal};

    #[actix_web::test]
    async fn library_pull_test() {
        println!("{:?}", library_pull_internal().await);
    }

    #[actix_web::test]
    async fn holder_query_test() {
        println!(
            "{:?}",
            holder_query_internal("9784001141276", &["Univ_Pu_Toyama", "Toyama_Takaoka"]).await
        );
    }
}

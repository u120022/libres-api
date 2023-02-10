use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BookChunk {
    pub items: Vec<Book>,
    pub total_count: u32,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Book {
    pub title: String,
    pub descriptions: Vec<String>,
    pub keywords: Vec<String>,
    pub creators: Vec<String>,
    pub publishers: Vec<String>,
    pub issued_at: Option<String>,
    pub isbn: Option<String>,
    pub language: Option<String>,
    pub annotations: Vec<String>,
    pub image_url: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LibraryChunk {
    pub items: Vec<Library>,
    pub total_count: u32,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    pub address: Option<String>,
    pub prefecture: Option<String>,
    pub city: Option<String>,
    pub postcode: Option<String>,
    pub tel: Option<String>,
    pub url: Option<String>,
    pub geocode: Option<(f64, f64)>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HolderChunk {
    pub items: Vec<Holder>,
    pub total_count: u32,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Holder {
    pub isbn: String,
    pub library_name: String,
    pub state: HolderState,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub enum HolderState {
    #[default]
    Nothing,
    Exists,
    Reserved,
    Borrowed,
    Inplace,
}

mod calil_api;
mod cinii_api;
mod entity;
mod models;
mod ndl_api;

use actix_web::{
    get, post,
    web::{route, Data, Json, Path, Query},
    App, HttpResponse, HttpServer,
};
use calil_api::CalilAppState;
use cinii_api::CiniiAppState;
use entity::Entity;
use ndl_api::NdlAppState;
use serde::Deserialize;
use std::{
    env::var,
    error::Error,
    net::{Ipv4Addr, SocketAddrV4},
};

type E = Box<dyn Error>;

#[actix_web::main]
async fn main() -> Result<(), E> {
    let port: u16 = var("FUNCTIONS_CUSTOMHANDLER_PORT")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);

    let entity_app_state = Entity::new(var("DATABASE_URL")?.as_str()).await?;
    let ndl_app_state = NdlAppState::new();
    let calil_app_state = CalilAppState::new(var("CALIL_APPKEY")?.as_str());
    let cinii_app_state = CiniiAppState::new(var("CINII_APPKEY")?.as_str());

    calil_app_state.pull_data().await?;

    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(entity_app_state.clone()))
            .app_data(Data::new(ndl_app_state.clone()))
            .app_data(Data::new(calil_app_state.clone()))
            .app_data(Data::new(cinii_app_state.clone()))
            .service(book_query)
            .service(book_get)
            .service(library_query)
            .service(library_geocode_query)
            .service(library_get)
            .service(holder_query)
            .service(holder_all_query)
            .service(user_create)
            .service(user_login)
            .service(user_logout)
            .service(user_get)
            .service(reserve_create)
            .service(reserve_query)
            .service(reserve_get)
            .default_service(route().to(fallback))
    })
    .bind(addr)?
    .run()
    .await?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct BookQuery {
    filter: String,
    page_size: u32,
    page: u32,
    backend: String,
}

#[get("/")]
async fn book_query(query: Query<BookQuery>, ndl: Data<NdlAppState>) -> HttpResponse {
    match query.backend.as_str() {
        "ndl" => {
            let Ok(result) = ndl.book_query(
                query.filter.as_str(),
                query.page_size,
                query.page
            ).await else {
                return HttpResponse::NotFound().body("failed to fetch data");
            };

            HttpResponse::Ok().json(result)
        }
        _ => HttpResponse::NotFound().body("invalid backend"),
    }
}

#[get("/book/{_}")]
async fn book_get(isbn: Path<String>, ndl: Data<NdlAppState>) -> HttpResponse {
    let Ok(result) = ndl.book_get(isbn.as_str()).await else {
        return HttpResponse::NotFound().body("failed to fetch data");
    };

    HttpResponse::Ok().json(result)
}

#[derive(Debug, Deserialize)]
struct LibraryQuery {
    prefecture: String,
    city: String,
    page_size: u32,
    page: u32,
}

#[get("/library")]
async fn library_query(query: Query<LibraryQuery>, calil: Data<CalilAppState>) -> HttpResponse {
    let Ok(result) = calil.library_query(
        query.prefecture.as_str(),
        query.city.as_str(),
        query.page_size,
        query.page
    ).await else {
        return HttpResponse::NotFound().body("failed to fetch data");
    };

    HttpResponse::Ok().json(result)
}

#[derive(Debug, Deserialize)]
struct LibraryGeocodeQuery {
    latitude: f64,
    longitude: f64,
    limit: u32,
}

#[get("/library_geocode")]
async fn library_geocode_query(
    query: Query<LibraryGeocodeQuery>,
    calil: Data<CalilAppState>,
) -> HttpResponse {
    let Ok(result) = calil.library_geocode_query(
        (query.latitude, query.longitude),
        query.limit
    ).await else {
        return HttpResponse::NotFound().body("failed to fetch data");
    };

    HttpResponse::Ok().json(result)
}

#[get("/library/{_}")]
async fn library_get(library_name: Path<String>, calil: Data<CalilAppState>) -> HttpResponse {
    let Ok(result) = calil.library_get(library_name.as_str()).await else {
        return HttpResponse::NotFound().body("failed to fetch data");
    };

    HttpResponse::Ok().json(result)
}

#[derive(Debug, Deserialize)]
struct HolderQuery {
    isbn: String,
    library_names: String,
}

#[get("/holder")]
async fn holder_query(query: Query<HolderQuery>, calil: Data<CalilAppState>) -> HttpResponse {
    let library_names: Vec<_> = query.library_names.split(',').collect();

    let Ok(result) = calil.holder_query(
        query.isbn.as_str(),
        &library_names
    ).await else {
        return HttpResponse::NotFound().body("failed to fetch data");
    };

    HttpResponse::Ok().json(result)
}

#[derive(Debug, Deserialize)]
struct HolderAllQuery {
    isbn: String,
    page_size: u32,
    page: u32,
}

#[get("/holder_all_query")]
async fn holder_all_query(
    query: Query<HolderAllQuery>,
    cinii: Data<CiniiAppState>,
) -> HttpResponse {
    let Ok(result) = cinii.holder_query(
        query.isbn.as_str(),
        query.page_size,
        query.page
    ).await else {
        return HttpResponse::NotFound().body("failed to fetch data");
    };

    HttpResponse::Ok().json(result)
}

#[derive(Debug, Deserialize)]
struct UserCreateData {
    email: String,
    password: String,
    fullname: String,
    address: String,
}

#[post("/user_create")]
async fn user_create(data: Json<UserCreateData>, entity: Data<Entity>) -> HttpResponse {
    let Ok(_) = entity.user_create(
        data.email.as_str(),
        data.password.as_str(),
        data.fullname.as_str(),
        data.address.as_str(),
    ).await else {
        return HttpResponse::Unauthorized().body("failed to login");
    };

    HttpResponse::Ok().body("success to create user")
}

#[derive(Debug, Deserialize)]
struct UserLoginData {
    email: String,
    password: String,
}

#[post("/user_login")]
async fn user_login(data: Json<UserLoginData>, entity: Data<Entity>) -> HttpResponse {
    let Ok(result) = entity.user_login(
        data.email.as_str(),
        data.password.as_str(),
    ).await else {
        return HttpResponse::Unauthorized().body("failed to login");
    };

    HttpResponse::Ok().json(result)
}

#[derive(Debug, Deserialize)]
struct TokenData {
    token: String,
}

#[post("/user_logout")]
async fn user_logout(data: Json<TokenData>, entity: Data<Entity>) -> HttpResponse {
    let Ok(_) = entity.user_logout(
        data.token.as_str(),
    ).await else {
        return HttpResponse::Unauthorized().body("failed to logout");
    };

    HttpResponse::Ok().body("success to logout")
}

#[post("/user_get")]
async fn user_get(data: Json<TokenData>, entity: Data<Entity>) -> HttpResponse {
    let Ok(result) = entity.user_get(
        data.token.as_str(),
    ).await else {
        return HttpResponse::Unauthorized().body("failed to logout");
    };

    HttpResponse::Ok().json(result)
}

#[derive(Debug, Deserialize)]
struct ReserveCreateData {
    token: String,
    isbn: String,
    library_name: String,
}

#[post("/reserve_create")]
async fn reserve_create(data: Json<ReserveCreateData>, entity: Data<Entity>) -> HttpResponse {
    let Ok(_) = entity.reserve_create(
        data.token.as_str(),
        data.isbn.as_str(),
        data.library_name.as_str(),
    ).await else {
        return HttpResponse::Unauthorized().body("failed to logout");
    };

    HttpResponse::Ok().body("success to create reserve")
}

#[derive(Debug, Deserialize)]
struct ReserveQueryData {
    token: String,
    page_size: u32,
    page: u32,
}

#[post("/reserve")]
async fn reserve_query(data: Json<ReserveQueryData>, entity: Data<Entity>) -> HttpResponse {
    let Ok(result) = entity.reserve_query(
        data.token.as_str(),
        data.page_size,
        data.page,
    ).await else {
        return HttpResponse::Unauthorized().body("failed to logout");
    };

    HttpResponse::Ok().json(result)
}

#[post("/reserve/{_}")]
async fn reserve_get(id: Path<u32>, data: Json<TokenData>, entity: Data<Entity>) -> HttpResponse {
    let Ok(result) = entity.reserve_get(
        data.token.as_str(),
        *id as i64,
    ).await else {
        return HttpResponse::Unauthorized().body("failed to logout");
    };

    HttpResponse::Ok().json(result)
}

async fn fallback() -> HttpResponse {
    HttpResponse::NotFound().body("no endpoint, but connection to api is successful.")
}

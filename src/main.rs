mod api;
mod models;

use actix_web::{main, web::route, App, HttpServer};
use api::{
    fallback, holder_query, library_geocode_query, library_get, library_query, new_state,
    pull_library,
};
use once_cell::sync::Lazy;
use std::{
    env::var,
    io::Result,
    net::{Ipv4Addr, SocketAddrV4},
};

static CALIL_APPKEY: Lazy<String> =
    Lazy::new(|| std::env::var("CALIL_APPKEY").expect("not found env var \"CALIL_APPKEY\""));

#[main]
async fn main() -> Result<()> {
    let port: u16 = var("FUNCTIONS_CUSTOMHANDLER_PORT")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);

    let state = new_state();

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(pull_library)
            .service(library_query)
            .service(library_geocode_query)
            .service(library_get)
            .service(holder_query)
            .default_service(route().to(fallback))
    })
    .bind(addr)?
    .run()
    .await
}

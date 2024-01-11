#![feature(try_trait_v2)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![warn(clippy::all, clippy::pedantic)]

use base64::Engine;
use color_eyre::eyre::{Ok, Result};
use futures::{Stream, StreamExt};
use k256::ecdsa::{SigningKey, VerifyingKey};
use poem::{
    handler,
    listener::TcpListener,
    middleware::Cors,
    web::{
        websocket::{Message, WebSocket, WebSocketStream},
        Data, Path,
    },
    EndpointExt, IntoResponse, Route, Server,
};
use poem_openapi::OpenApiService;
use sqlx::mysql::MySqlPoolOptions;
use std::{collections::HashMap, sync::Arc};
use table::GameAPI;
use tanktacticsgame::{Settings, BASE64};
use tokio::sync::Mutex;

mod table;

#[allow(clippy::needless_pass_by_value)]
#[handler]
fn index(
    Path(name): Path<String>,
    ws: WebSocket,
    connections: Data<&Arc<Mutex<HashMap<i32, WebSocketStream>>>>,
) -> impl IntoResponse {
    let p = connections.0.clone();
    ws.protocols(vec!["tanktacktics"])
        .on_upgrade(|mut socket| async move {
            let mut lock = p.lock().await;
            if let Result::Ok(val) = name.parse() {
                lock.insert(val, socket);
            }
        })
}

#[tokio::main]
async fn main() -> Result<()> {
    let pairs = unsafe {
        let keys_str = std::str::from_utf8_unchecked(include_bytes!("..\\secret.txt"))
            .split_once('\n')
            .unwrap();
        (
            SigningKey::from_slice(&BASE64.decode(keys_str.0.trim_matches('\r')).unwrap()).unwrap(),
            keys_str.1,
        )
    };

    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        //.connect("mysql://server:Jako0101!@192.168.2.16/mysql")
        .connect("mysql://root:jako0101@localhost:3306/mysql")
        .await?;

    let connections = Arc::new(Mutex::new(HashMap::<i32, WebSocketStream>::new()));
    let ws = Route::new().at("/:name", poem::get(index));
    let api_service =
        OpenApiService::new(GameAPI, "Game API", "1.0").server("http://localhost:3000");
    let ui = api_service.swagger_ui();
    let app = Route::new()
        .nest("/", api_service)
        .nest("/docs", ui)
        .nest("/ws", ws)
        .data(pool)
        .data(connections)
        .data(pairs)
        .with(Cors::new());

    Server::new(TcpListener::bind("127.0.0.1:3000"))
        .run(app)
        .await?;

    Ok(())
}

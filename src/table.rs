use base64::Engine;
use futures::{SinkExt, StreamExt};
use k256::{
    ecdh::EphemeralSecret,
    ecdsa::{
        signature::{Signer, Verifier},
        Signature, SigningKey, VerifyingKey,
    },
    EncodedPoint,
};
use poem::{
    error::{NotFoundError, ResponseError},
    http::StatusCode,
    listener::TcpListener,
    web::{
        websocket::{Message, WebSocketStream},
        Data,
    },
    Route, Server,
};
use poem_openapi::{
    error::ParseParamError,
    param::Query,
    payload::{Form, Json, PlainText, Response},
    types::{ParseFromJSON, ParseFromParameter, ToJSON, Type},
    ApiResponse, Enum, Object, OpenApi, OpenApiService,
};
use rand_chacha::rand_core::OsRng;
use serde::Deserialize;
use serde_json::Value;
use sqlx::{mysql::MySqlPool, query};
use std::{
    collections::{HashMap, HashSet},
    ops::Try,
    process::Output,
    str::FromStr,
    sync::Arc,
};
use tanktacticsgame::{
    get_key, DataBaseGame, Game, LevelRangeMap, MoveLine, Settings, User, BASE64,
};
use thiserror::Error;
use tokio::sync::Mutex;

pub struct GameAPI;

#[derive(Object)]
struct SignedData {
    /// The data. (Either a private key or random data. Both encrypted.)
    data: String,
    /// The server signature.
    signature: String,
}
#[derive(Debug, Clone, Enum)]
pub enum SignalType {
    SendRandom,
    SendKey,
}
#[derive(ApiResponse)]
enum CustomResponse<T: Type + ToJSON> {
    /// Request was successful.
    #[oai(status = 200)]
    Ok(Json<T>),
    /// A rule error occured in the game.
    #[oai(status = 400)]
    UserError(PlainText<String>),
    /// An error occured during the database lookup.
    #[oai(status = 500)]
    ServerError(PlainText<String>),
}
impl<T: Type + ToJSON> CustomResponse<T> {
    fn error(text: &str, server: bool) -> CustomResponse<T> {
        if server {
            CustomResponse::ServerError(PlainText(text.into()))
        } else {
            CustomResponse::UserError(PlainText(text.into()))
        }
    }
}
impl<T: Type + ToJSON> Try for CustomResponse<T> {
    type Output = T;

    type Residual = CustomResponse<T>;

    fn from_output(output: Self::Output) -> Self {
        CustomResponse::Ok(Json(output))
    }

    fn branch(self) -> std::ops::ControlFlow<Self::Residual, Self::Output> {
        match self {
            CustomResponse::Ok(Json(v)) => std::ops::ControlFlow::Continue(v),
            _ => std::ops::ControlFlow::Break(self),
        }
    }
}
impl<T: Type + ToJSON, P: Type + ToJSON> std::ops::FromResidual<CustomResponse<T>>
    for CustomResponse<P>
{
    fn from_residual(residual: CustomResponse<T>) -> Self {
        match residual {
            CustomResponse::Ok(_) => panic!(),
            CustomResponse::UserError(s) => CustomResponse::UserError(s),
            CustomResponse::ServerError(s) => CustomResponse::ServerError(s),
        }
    }
}
impl<T: Type + ToJSON> std::ops::FromResidual<Result<std::convert::Infallible, CustomResponse<T>>>
    for CustomResponse<T>
{
    fn from_residual(residual: Result<std::convert::Infallible, CustomResponse<T>>) -> Self {
        residual.err().unwrap()
    }
}

#[OpenApi]
impl GameAPI {
    /// Returns the last token from a game specified by the `game` query. Gives a server error if the game is corrupted.
    #[oai(path = "/head", method = "get")]
    async fn get_head(
        &self,
        pool: Data<&MySqlPool>,
        Query(game): Query<i32>,
    ) -> CustomResponse<String> {
        let Ok(record) = sqlx::query!("SELECT token FROM moves WHERE moves.game = ? AND `index` = (SELECT MAX(`index`) FROM moves WHERE moves.game = ?);", game, game)
            .fetch_one(pool.0)
            .await
        else { return CustomResponse::Ok(Json(String::new())); };
        match MoveLine::parse_from_json_string(&record.token) {
            Ok(line) => CustomResponse::Ok(Json(line.signature)),
            Err(_) => CustomResponse::error("Corrupted game.", true),
        }
    }
    /// Returns all active games and their settings.
    #[oai(path = "/games", method = "get")]
    async fn get_games(&self, pool: Data<&MySqlPool>) -> Json<Vec<DataBaseGame>> {
        let games = sqlx::query_as!(DataBaseGame, "SELECT * FROM games")
            .fetch_all(pool.0)
            .await
            .unwrap_or_default();
        Json(games)
    }
    /// Gets all signed moves for a specific game. Gives a server error if a move has been corrupted.
    #[oai(path = "/tokens", method = "get")]
    async fn get_tokens(
        &self,
        pool: Data<&MySqlPool>,
        Query(game): Query<i32>,
    ) -> CustomResponse<Vec<MoveLine>> {
        let lines = sqlx::query!(
            "SELECT moves.token FROM moves WHERE moves.game = ? ORDER BY moves.index;",
            game
        )
        .fetch_all(pool.0)
        .await
        .into_iter()
        .flat_map(Vec::into_iter)
        .map(|r| MoveLine::parse_from_json_string(&r.token))
        .try_fold(Vec::new(), |mut x, y| {
            y.as_ref().ok()?;
            x.extend(y);
            Some(x)
        })
        .ok_or(CustomResponse::error("Corrupted move.", true))?;
        CustomResponse::Ok(Json(lines))
    }
    /// Gets the public key of all players in a specific game.
    #[oai(path = "/users", method = "get")]
    async fn get_users(&self, pool: Data<&MySqlPool>, Query(game): Query<i32>) -> Json<Vec<User>> {
        let users = sqlx::query_as!(User, "SELECT DISTINCT users.id, users.public_key FROM moves, users WHERE moves.game = ? AND moves.user = users.id", game)
            .fetch_all(pool.0)
            .await
            .unwrap_or_default();
        Json(users)
    }
    /// Make a move. Gives a server error if a move, a user key or a game has been corrupted. Gives a user error if the game does not exist
    #[oai(path = "/move", method = "post")]
    async fn make_move(
        &self,
        pool: Data<&MySqlPool>,
        Query(game): Query<i32>,
        Json(token): Json<MoveLine>,
    ) -> CustomResponse<i32> {
        let game_id = game;
        let mut users = self
            .get_users(Data(pool.0), Query(game_id))
            .await
            .0
            .into_iter()
            .map(|user| get_key(user.public_key).map(|x| (user.id, x)))
            .try_fold(HashMap::new(), |mut x, y| {
                x.extend(y);
                y.map(|_| x)
            })
            .ok_or(CustomResponse::error("Corrupted user key.", true))?;
        if !users.contains_key(&token.authorizer) {
            let record = query!(
                "SELECT public_key FROM users WHERE id = ?",
                token.authorizer
            )
            .fetch_one(pool.0)
            .await
            .ok()
            .and_then(|r| get_key(r.public_key))
            .map(|x| (token.authorizer, x));
            users.extend(record);
        }
        let tokens = self.get_tokens(Data(pool.0), Query(game_id)).await?;
        let len: i32 = tokens.len().try_into().unwrap();
        let mut game = sqlx::query_as!(
            DataBaseGame,
            "SELECT * FROM games WHERE games.id = ?",
            game_id
        )
        .fetch_one(pool.0)
        .await
        .map_err(|_| CustomResponse::error("Game does not exist.", false))?
        .as_game(tokens, &users)
        .map_err(|e| CustomResponse::error(&format!("Corrupted game: {e}."), true))?;
        game.load(token.clone(), &users)
            .map_err(|e| CustomResponse::error(&format!("Malformed line given: {e}."), false))?;
        match sqlx::query!(
            "INSERT INTO moves VALUES (?, ?, ?, ?);",
            token.authorizer as i32,
            game_id,
            len,
            token.to_json_string()
        )
        .execute(pool.0)
        .await
        {
            Ok(r) => CustomResponse::Ok(Json(r.last_insert_id().try_into().unwrap())),
            Err(e) => CustomResponse::error(&format!("SQL error: {e}."), true),
        }
    }
    /// Regester a new user with a public key. Returns the id of the new user. Game error and not found error should never be returned. DataBaseError GameError
    #[oai(path = "/regester", method = "post")]
    async fn regester(
        &self,
        pool: Data<&MySqlPool>,
        Json(public_key): Json<String>,
    ) -> CustomResponse<i32> {
        get_key(public_key.clone()).ok_or(CustomResponse::error("Malformed key given.", false))?;
        sqlx::query!("INSERT INTO users (public_key) VALUES (?);", public_key)
            .execute(pool.0)
            .await
            .map_err(|e| CustomResponse::error(&format!("SQL error: {e}."), true))
            .map(|r| CustomResponse::Ok(Json(r.last_insert_id().try_into().unwrap())))?
    }
    /// Create a new game with settings. Returns the id of the new game. Game error and not found error should never be returned. GameError
    #[oai(path = "/make_game", method = "post")]
    async fn make_game(
        &self,
        pool: Data<&MySqlPool>,
        Json(game): Json<DataBaseGame>,
    ) -> CustomResponse<i32> {
        LevelRangeMap::from_str(game.range.as_str())
            .map_err(|_| CustomResponse::error("Malformed range map given.", false))?;
        let p = sqlx::query!("INSERT INTO games (seed, width, height, health, max_level, max_players, vote_threshold, `range`, last_vote) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?);", game.seed, game.width, game.height, game.health, game.max_level, game.max_players, game.vote_threshold, game.range, game.last_vote).execute(pool.0)
            .await
            .map_err(|e| CustomResponse::error(&format!("SQL error: {e}."), true))?;
        CustomResponse::Ok(Json(p.last_insert_id().try_into().unwrap()))
    }
    /// Sends Signal to the client. Either it is a request for random data or a request for the users private key. The private key will only be returned if the clients have confirmed security using a random packet.
    #[oai(path = "/sendclient", method = "post")]
    async fn sendclient(
        &self,
        connections: Data<&Arc<Mutex<HashMap<i32, WebSocketStream>>>>,
        pool: Data<&MySqlPool>,
        keys: Data<&(SigningKey, &'static str)>,
        Query(user): Query<i32>,
        Query(message): Query<SignalType>,
        Json(mut encryption_key): Json<String>,
    ) -> CustomResponse<SignedData> {
        let public_key = query!("SELECT public_key FROM users WHERE id = ?", user)
            .fetch_one(pool.0)
            .await
            .ok()
            .and_then(|x| get_key(x.public_key))
            .ok_or(CustomResponse::error("User not availible.", false))?;

        let mut lock = connections.0.lock().await;
        let user = lock
            .get_mut(&user)
            .ok_or(CustomResponse::error("User not availible.", false))?;
        let (data, signature) = async {
            encryption_key.push_str(&message.to_json_string());
            user.send(poem::web::websocket::Message::Text(encryption_key))
                .await
                .ok()?;
            let data = user
                .next()
                .await
                .and_then(Result::ok)
                .and_then(|x| match x {
                    Message::Text(s) => Some(s),
                    _ => None,
                });
            let signature = user
                .next()
                .await
                .and_then(Result::ok)
                .and_then(|x| match x {
                    Message::Text(s) => Some(s),
                    _ => None,
                })
                .map(|x| BASE64.decode(x))
                .and_then(Result::ok)
                .as_deref()
                .map(Signature::from_slice)
                .and_then(Result::ok);
            data.zip(signature)
        }
        .await
        .ok_or(CustomResponse::error("Connection issue.", true))?;
        public_key
            .verify(data.as_bytes(), &signature)
            .map_err(|_| CustomResponse::error("Connection not secure.", true))?;
        let signature: Signature = keys.0 .0.sign(data.as_bytes());
        let signature = BASE64.encode(signature.to_bytes().as_slice());
        CustomResponse::Ok(Json(SignedData { data, signature }))
    }
}

use std::collections::HashMap;

use frontend::{get_json, get_text, request};
use sycamore::reactive::{use_context, Scope, Signal};
use tanktacticsgame::{get_key, DataBaseGame, Game, MoveLine, User};
use web_sys::{Response, Storage};

pub async fn get_games() -> Result<Vec<DataBaseGame>, ()> {
    let response = request("GET", "/games".into(), HashMap::new(), None).await?;
    let value: Vec<DataBaseGame> = get_json(response).await?;
    Ok(value)
}
pub async fn send_move(private_key: String, game: i32, mut line: MoveLine) -> Result<Response, ()> {
    let head = request("GET", format!("/head?game={game}"), HashMap::new(), None).await?;
    let head = get_text(head).await?;
    let head = head.trim_matches('"');
    line.sign(Some(head), private_key).unwrap();
    let mut headers = HashMap::new();
    headers.insert("Content-Type".into(), "application/json".into());
    request(
        "POST",
        format!("/move?game={game}"),
        headers,
        Some(serde_json::to_string(&line).unwrap()),
    )
    .await
}
pub async fn get_game(game: DataBaseGame) -> Result<(Game, Vec<MoveLine>), ()> {
    let users = request(
        "GET",
        format!("/users?game={}", game.id),
        HashMap::new(),
        None,
    )
    .await?;
    let users = get_json::<Vec<User>>(users)
        .await?
        .into_iter()
        .map(|x| (x.id, get_key(x.public_key).unwrap()))
        .collect::<HashMap<_, _>>();

    let tokens = request(
        "GET",
        format!("/tokens?game={}", game.id),
        HashMap::new(),
        None,
    )
    .await?;
    let tokens = get_json::<Vec<MoveLine>>(tokens).await?;
    let game = game.as_game(tokens.clone(), &users).map_err(|_| ())?;
    Ok((game, tokens))
}
pub async fn join_game(cx: Scope<'_>, game: DataBaseGame) -> Result<(), ()> {
    let storage = use_context::<Signal<Storage>>(cx);
    let private_key = storage.get().get_item("private_key").unwrap().unwrap(); // JS function doesnt panic | join game only called when regestered
    let user = storage
        .get()
        .get_item("user")
        .unwrap()
        .unwrap()
        .parse::<i32>()
        .unwrap(); // JS function doesnt panic | join game only called when regestered | user is always a number

    let game = get_game(game).await?.0;

    let (x, y) = game.get_pos();

    let m = MoveLine {
        move_type: tanktacticsgame::MoveLineType::Join,
        x: Some(x),
        y: Some(y),
        target: None,
        authorizer: user,
        signature: String::new(),
    };

    send_move(private_key, game.id, m).await?;

    storage
        .get()
        .set_item("game", &game.id.to_string())
        .unwrap();
    Ok(())
}

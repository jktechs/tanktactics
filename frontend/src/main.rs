#![warn(clippy::all, clippy::pedantic)]

use frontend::{get_text, request};
use js_sys::eval;
use std::collections::HashMap;
use std::str::FromStr;
use sycamore::futures::spawn_local_scoped;
use sycamore::prelude::*;
use tanktacticsgame::{get_random_keys, Game, MoveLine};
use web_sys::{window, Storage, WebSocket};

use crate::api::{get_game, get_games, join_game, send_move};

mod api;

#[derive(Prop)]
struct WorldProps<'a> {
    user: i32,
    game: &'a ReadSignal<(Game, Vec<MoveLine>)>,
}
#[component]
fn World<'a, G: Html>(cx: Scope<'a>, WorldProps { game, user }: WorldProps<'a>) -> View<G> {
    let x = create_signal(cx, 0);
    let y = create_signal(cx, 0);
    let target = create_signal(cx, 0);
    let shoot = create_signal(cx, false);
    let drive = create_signal(cx, false);
    let vote = create_signal(cx, false);

    let width = game.get().0.settings.width;
    let height = game.get().0.settings.height;
    let tokens = game.map(cx, |x| {
        x.1.iter()
            .map(MoveLine::clone)
            .enumerate()
            .collect::<Vec<_>>()
    });
    let board = game.map(cx, |game| game.0.board.clone());
    let count = create_signal(cx, (0..(width * height)).collect::<Vec<_>>());
    let token_string = tokens
        .get()
        .iter()
        .map(|x| x.1.to_string())
        .collect::<Vec<_>>()
        .join("\\n");
    view!(cx,
        div(id="tokens") {
            Keyed(
                iterable=tokens,
                view=move |cx, x| {
                    let m = x.1.to_string().split_once('|').unwrap().0.to_string();
                    view! { cx,
                        div(class="token") {
                            (m)
                        }
                    }
                },
                key=|x| x.0,
            )
            button(on:click=move |_| {
                let _ = eval(&format!("window.navigator.clipboard.writeText(\"{token_string}\")"));
            }) {
                "Copy"
            }
        }
        div(id="world", style={format!("width:{}px;height:{}px", width * 50, height * 50)}) {
            Keyed(
                iterable=count,
                view=move |cx, i| view! { cx,
                    div(on:click=move |_| {
                        x.set(i % width);
                        y.set(i / width);

                        let game = game.get();
                        let player = game.0.players.get(&user).unwrap();
                        let is_tank = game.0.board.get(&(i % width, i / width));
                        if let Some(id) = is_tank {
                            target.set(*id);
                        }
                        let target = is_tank.map(|id| game.0.players.get(id).unwrap());
                        let target_alive = target.map_or(false, |x| x.is_alive(true).is_ok());

                        shoot.set(player.is_alive(true).is_ok() && target_alive && player.in_range(i % width, i / width, game.0.settings.range.get_range(player.level)).is_ok());
                        drive.set(player.is_alive(true).is_ok() && is_tank.is_none() && player.in_range(i % width, i / width, 1).is_ok());
                        vote.set(player.is_alive(false).is_ok() && target_alive);
                    }, class={
                        if let Some(p) = board.get().get(&(i % width, i / width)) {
                            if p == &user {
                                "tile user"
                            } else {
                                "tile player"
                            }
                        } else {
                            "tile"
                        }
                    }, style={format!("left:{}px;top:{}px", (i % width) * 50, (i / width) * 50)}) {
                        ({
                            board.get().get(&(i % width, i / width)).map_or(String::default(), std::string::ToString::to_string)
                        })
                    }
                },
                key=|x| *x,
            )
            ContextMenu(shoot=shoot, vote=vote, drive=drive, x=x, y=y, target=target, user=user, game=game.get().0.id)
        }
    )
}
#[derive(Prop)]
struct ContextMenuProps<'a> {
    shoot: &'a Signal<bool>,
    vote: &'a Signal<bool>,
    drive: &'a Signal<bool>,
    x: &'a Signal<u32>,
    y: &'a Signal<u32>,
    target: &'a Signal<i32>,
    user: i32,
    game: i32,
}
#[component]
fn ContextMenu<'a, G: Html>(
    cx: Scope<'a>,
    ContextMenuProps {
        shoot,
        vote,
        drive,
        user,
        game,
        x,
        y,
        target,
    }: ContextMenuProps<'a>,
) -> View<G> {
    let storage = use_context::<Signal<Storage>>(cx);
    view!(cx,
        div(id="modal", style={format!("left:{}px;top:{}px",25+<u32 as TryInto<i32>>::try_into(*x.get()).unwrap()*50i32,<u32 as TryInto<i32>>::try_into(*y.get()).unwrap()*50i32-25)}) {
            (if *shoot.get() {view!(cx,
                button(style="display:block", on:click=move |_| {
                    let private_key = storage.get().get_item("private_key").unwrap().unwrap();
                    let line = MoveLine {authorizer: user, move_type: tanktacticsgame::MoveLineType::Shoot, signature: String::new(), target: Some(*target.get()), x: None, y: None};
                    spawn_local_scoped(cx, async move {send_move(private_key, game, line).await.unwrap();storage.trigger_subscribers();});
                }) {"Shoot"}
            )} else {view!(cx,)})
            (if *drive.get() {view!(cx,
                button(style="display:block", on:click=move |_| {
                    let private_key = storage.get().get_item("private_key").unwrap().unwrap();
                    let line = MoveLine {authorizer: user, move_type: tanktacticsgame::MoveLineType::Drive, signature: String::new(), target: None, x: Some(*x.get()), y: Some(*y.get())};
                    spawn_local_scoped(cx, async move {send_move(private_key, game, line).await.unwrap();storage.trigger_subscribers();});
                }) {"Move"}
            )} else {view!(cx,)})
            (if *vote.get() {view!(cx,
                button(style="display:block", on:click=move |_| {
                    let private_key = storage.get().get_item("private_key").unwrap().unwrap();
                    let line = MoveLine {authorizer: user, move_type: tanktacticsgame::MoveLineType::Vote, signature: String::new(), target: Some(*target.get()), x: None, y: None};
                    spawn_local_scoped(cx, async move {send_move(private_key, game, line).await.unwrap();storage.trigger_subscribers();});
                }) {"Vote"}
            )} else {view!(cx,)})
        }
    )
}
#[component]
fn Hud<G: Html>(cx: Scope, height: u32) -> View<G> {
    let storage = use_context::<Signal<Storage>>(cx);
    let delete_keys = |_| {
        storage.get().remove_item("game").unwrap(); // JS function doesnt panic
        storage.get().remove_item("public_key").unwrap(); // JS function doesnt panic
        storage.get().remove_item("private_key").unwrap(); // JS function doesnt panic
        storage.trigger_subscribers();
    };
    view!(cx,
        div(id="hud",style={format!("height:{}px", height * 50)}) {
            button(on:click=delete_keys) {"Delete Account from device."}
        }
    )
}
#[component]
async fn GameList<G: Html>(cx: Scope<'_>) -> View<G> {
    let games = create_signal(cx, get_games().await.unwrap_or_default());
    view!(cx,
        table {
            tr {
                th {"Join"}
                th {"Id"}
                th {"Seed"}
                th {"Width"}
                th {"Height"}
                th {"Health"}
                th {"Max Level"}
                th {"Max Players"}
                th {"Vote Threshold"}
                th {"Range"}
                th {"Last Vote"}
            }
            Keyed(
                iterable=games,
                view=|cx, x| {
                    let cloned_x = x.clone();
                    view! { cx,
                        tr {
                            td { button(on:click=move |_| {
                                let cloned_x = cloned_x.clone();
                                spawn_local_scoped(cx, async move {join_game(cx, cloned_x.clone()).await.unwrap();use_context::<Signal<Storage>>(cx).trigger_subscribers()});
                            }) {"join"} }
                            td { (x.id) }
                            td { (x.seed) }
                            td { (x.width) }
                            td { (x.height) }
                            td { (x.health) }
                            td { (x.max_level) }
                            td { (x.max_players) }
                            td { (x.vote_threshold) }
                            td { (x.range) }
                            td { (x.last_vote) }
                        }
                    }
                },
                key=|x| x.id,
            )
        }
        button(on:click=move |_| spawn_local_scoped(cx, async move {
            let list = get_games().await.unwrap_or_default();
            games.set(list);
        })) {"Refresh"}
    )
}
async fn regester(cx: Scope<'_>, key: String) -> Result<(), ()> {
    let mut headers = HashMap::new();
    headers.insert("Content-Type".into(), "application/json".into());
    let response = request(
        "POST",
        "/regester".into(),
        headers,
        Some(format!("\"{key}\"")),
    )
    .await?;
    let response = get_text(response).await?;

    let storage = use_context::<Signal<Storage>>(cx);
    storage.get().set_item("user", &response).unwrap();
    Ok(())
}
#[component]
fn Login<G: Html>(cx: Scope) -> View<G> {
    let storage = use_context::<Signal<Storage>>(cx);
    let public_key = create_memo(cx, || storage.get().get_item("public_key").unwrap());

    let deleteKeys = |_| {
        storage.get().remove_item("game").unwrap();
        storage.get().remove_item("public_key").unwrap();
        storage.get().remove_item("private_key").unwrap();
        storage.trigger_subscribers();
    };
    view!(
        cx,
        (if public_key.get().is_some() {
            view! { cx,
                button(on:click=deleteKeys) { "Delete Account from device." }
                br()
                "Please join a game."
                GameList()
            }
        } else {
            view! { cx, button(on:click= move |_| spawn_local_scoped(cx, async move {
                let (private, public) = get_random_keys();
                storage.get().set_item("public_key", &public).unwrap();
                storage.get().set_item("private_key", &private).unwrap();
                regester(cx, public).await.unwrap();
                storage.trigger_subscribers();
            })) { "Generate new Account" } }
        })
    )
}
#[component]
async fn Game<G: Html>(cx: Scope<'_>, game: i32) -> View<G> {
    let storage = use_context::<Signal<Storage>>(cx);
    let user: i32 = storage
        .get()
        .get_item("user")
        .unwrap()
        .unwrap()
        .parse()
        .unwrap();

    let game = get_games()
        .await
        .unwrap()
        .into_iter()
        .find(|x| x.id == game)
        .unwrap();
    let game = get_game(game).await.unwrap();
    let game = create_signal(cx, game);

    view!(cx,
        World(user=user, game=game)
        Hud(game.get().0.settings.height)
    )
}
fn main() {
    let storage = window().unwrap().local_storage().unwrap().unwrap();
    let user = storage
        .get_item("user")
        .unwrap()
        .as_deref()
        .map(<i32 as FromStr>::from_str)
        .and_then(Result::ok);
    sycamore::render(|cx| {
        let storage = create_signal(cx, storage);
        provide_context_ref(cx, storage);
        if let Some(user) = user {
            let socket =
                WebSocket::new_with_str(&format!("ws://127.0.0.1:3000/ws/{user}"), "tanktacktics")
                    .unwrap();
            let socket = create_signal(cx, socket);
            provide_context_ref(cx, socket);
        }
        view!(
            cx,
            (if let Some(game) = storage
                .get()
                .get_item("game")
                .unwrap()
                .as_deref()
                .map(<i32 as FromStr>::from_str)
                .and_then(Result::ok)
            {
                Game(cx, game)
            } else {
                Login(cx)
            })
        )
    });
}

#![warn(clippy::all, clippy::pedantic)]

use base64::{
    alphabet::URL_SAFE,
    engine::{GeneralPurpose, GeneralPurposeConfig},
    Engine,
};
use k256::ecdsa::{signature::Signer, signature::Verifier, Signature, SigningKey, VerifyingKey};
#[cfg(feature = "openapi")]
use poem_openapi::{self, Enum, Object};
use rand_chacha::rand_core::{OsRng, RngCore, SeedableRng};
use std::{collections::HashMap, fmt::Display /* time::SystemTime, */, usize};

pub const BASE64: GeneralPurpose = GeneralPurpose::new(&URL_SAFE, GeneralPurposeConfig::new());

pub fn get_random_keys() -> (String, String) {
    let signing_key = SigningKey::random(&mut OsRng);
    let verifying_key = VerifyingKey::from(&signing_key);
    (
        BASE64.encode(signing_key.to_bytes().as_slice()),
        BASE64.encode(verifying_key.to_encoded_point(true).as_bytes()),
    )
}
pub fn get_key(key: String) -> Option<VerifyingKey> {
    BASE64
        .decode(key)
        .ok()
        .as_deref()
        .map(VerifyingKey::from_sec1_bytes)
        .and_then(Result::ok)
}

#[cfg_attr(feature = "openapi", derive(Object))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Hash, PartialEq, Eq)]
pub struct User {
    /// The user id.
    pub id: i32,
    /// The user public key.
    pub public_key: String,
}
pub struct Player {
    pub user: i32,
    pub x: u32,
    pub y: u32,
    pub level: u32,
    pub points: u32,
    pub health: u32,
}
impl Player {
    /// Check the alive state of the player.
    /// Does nothing and returns `Result::Ok()` if the states match.
    /// # Errors
    /// If the states do not match.
    pub fn is_alive(&self, alive: bool) -> Result<(), Error> {
        if (self.health != 0) == alive {
            Ok(())
        } else {
            Err(Error::OutOfRange(
                "Health".into(),
                if alive { "> 0" } else { " == 0" }.into(),
            ))
        }
    }
    /// Check if the player has at least one point.
    /// Does nothing and returns `Result::Ok()` if the player has points.
    /// # Errors
    /// If the player has no points.
    pub fn has_points(&self) -> Result<(), Error> {
        if self.points == 0 {
            Err(Error::OutOfRange("Points".into(), "> 0".into()))
        } else {
            Ok(())
        }
    }
    /// Check if the position (`x`,`y`) is in the range of `distance`.
    /// Does nothing and returns `Result::Ok()` if the player is in range.
    /// # Errors
    /// If the player is not in range.
    pub fn in_range(&self, x: u32, y: u32, distance: u32) -> Result<(), Error> {
        if self.x.abs_diff(x).max(self.y.abs_diff(y)) > distance {
            Err(Error::OutOfRange(
                "Position".into(),
                format!("distance <= {distance}"),
            ))
        } else {
            Ok(())
        }
    }
    /// Check if the player can upgrade their tank.
    /// Does nothing and returns `Result::Ok()` if the player can upgrade.
    /// # Errors
    /// If the player is at max level.
    pub fn upgradable(&self, max_level: i32) -> Result<(), Error> {
        if <u32 as TryInto<i32>>::try_into(self.level).map_or(true, |x| x == max_level) {
            Err(Error::OutOfRange("Level".into(), format!("< {max_level}")))
        } else {
            Ok(())
        }
    }
}
#[cfg_attr(feature = "openapi", derive(Enum))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum MoveLineType {
    /// Join at position.
    Join,
    /// Move to target. (uses one point)
    Drive,
    /// Shoot at target to decrement health. (uses one point)
    Shoot,
    /// Gift target a point. (uses one point)
    Gift,
    /// Vote for a target to gain a point.
    Vote,
    /// Count all votes and distribute points. (giving all players exceeding the threshold an extra point)
    HandleVotes,
    /// Go up a level. (uses one point)
    Upgrade,
}
#[cfg_attr(feature = "openapi", derive(Object))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct MoveLine {
    /// The type of move.
    pub move_type: MoveLineType,
    /// The x position of the move.
    pub x: Option<u32>,
    /// The y position of the move.
    pub y: Option<u32>,
    /// The target of the move.
    pub target: Option<i32>,
    /// The user that authorized this move.
    pub authorizer: i32,
    /// The move signed by the authorizer.
    pub signature: String,
}
impl MoveLine {
    /// Calculates the signature of this move and stores it in the signature field.
    /// # Errors
    /// If the `private_key` is not correctly formated (url safe base 64 string of a point on the k256 curve).
    pub fn sign(&mut self, last: Option<&str>, private_key: String) -> Result<(), Error> {
        let key = BASE64
            .decode(private_key)
            .ok()
            .and_then(|x| SigningKey::from_slice(x.as_slice()).ok())
            .ok_or_else(|| Error::Other("Malformed private key.".into()))?;
        self.signature = String::default();
        let mut data = self.to_string();
        if let Some(last) = last {
            data.push_str(last);
        }
        let signature: Signature = key.sign(data.as_bytes());
        self.signature = signature.to_string();
        Ok(())
    }
}
impl Display for MoveLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.authorizer)?;

        match self.move_type {
            MoveLineType::Join => write!(
                f,
                "J{},{}",
                self.x.ok_or(std::fmt::Error)?,
                self.y.ok_or(std::fmt::Error)?
            ),
            MoveLineType::Drive => write!(
                f,
                "D{},{}",
                self.x.ok_or(std::fmt::Error)?,
                self.y.ok_or(std::fmt::Error)?
            ),
            MoveLineType::Shoot => write!(f, "S{}", self.target.ok_or(std::fmt::Error)?),
            MoveLineType::Gift => write!(f, "G{}", self.target.ok_or(std::fmt::Error)?),
            MoveLineType::Vote => write!(f, "V{}", self.target.ok_or(std::fmt::Error)?),
            MoveLineType::HandleVotes => write!(f, "H"),
            MoveLineType::Upgrade => write!(f, "U"),
        }?;
        write!(f, "|{}", self.signature)
    }
}
#[cfg_attr(feature = "openapi", derive(Object))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[derive(Clone, PartialEq)]
pub struct DataBaseGame {
    /// The game id.
    pub id: i32,
    /// The seed used for random actions. (mainly used for initial position)
    pub seed: u64,
    /// The unix time at which the last vote was called
    pub last_vote: u64,
    /// The width of the board.
    pub width: u32,
    /// The height of the board.
    pub height: u32,
    /// The initial health of all players.
    pub health: u32,
    /// The max level players can reach.
    pub max_level: i32,
    /// The max amount of players.
    pub max_players: i32,
    /// The minimum amount of votes needed for a point.
    pub vote_threshold: u32,
    /// The method for calculating the range from the level.
    pub range: String,
}
impl DataBaseGame {
    /// Gets the actual game without any moves from the database item.
    /// # Errors
    /// If the `LevelRangeMap` is not correctly formatted.
    pub fn as_game(
        self,
        moves: Vec<MoveLine>,
        users: &HashMap<i32, VerifyingKey>,
    ) -> Result<Game, Error> {
        let Ok(range) = self.range.parse::<LevelRangeMap>() else {
            return Err(Error::Other("Malformed LevelRangeMap.".into()));
        };
        let mut game = Game::new(
            self.id,
            Settings {
                health: self.health,
                width: self.width,
                height: self.height,
                max_level: self.max_level,
                max_players: self.max_players,
                vote_threshold: self.vote_threshold,
                seed: self.seed,
                range,
            },
        );
        for m in moves {
            game.load(m, users)?;
        }
        Ok(game)
    }
}
pub struct Game {
    pub id: i32,
    pub last_vote: u64,
    pub settings: Settings,
    pub players: HashMap<i32, Player>,
    pub board: HashMap<(u32, u32), i32>,
    pub votes: HashMap<i32, i32>,
    pub lines: Vec<MoveLine>,
    pub rand: rand_chacha::ChaCha12Rng,
}
impl Game {
    #[must_use]
    pub fn new(id: i32, settings: Settings) -> Self {
        //let unix = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        Game {
            rand: rand_chacha::ChaCha12Rng::seed_from_u64(settings.seed),
            id,
            players: HashMap::new(),
            board: HashMap::new(),
            votes: HashMap::new(),
            lines: Vec::new(),
            settings,
            last_vote: 0,
        }
    }
    fn get_player(&self, id: i32) -> Result<&Player, Error> {
        if let Some(player) = self.players.get(&id) {
            Ok(player)
        } else {
            Err(Error::NotFound(format!("player ({id})")))
        }
    }
    /// Load a `MoveLine` into the game object.
    /// # Errors
    /// * If the `line` is in any way invalid.
    /// * If the signature of the user is invalid. (url safe base 64 string of a point on the k256 curve)
    pub fn load(
        &mut self,
        mut line: MoveLine,
        users: &HashMap<i32, VerifyingKey>,
    ) -> Result<(), Error> {
        let mut signature = String::new();
        std::mem::swap(&mut signature, &mut line.signature);

        let mut data = line.to_string();
        if let Some(last) = self.lines.last() {
            data.push_str(&last.signature.to_string());
        }
        signature
            .parse::<Signature>()
            .ok()
            .and_then(|s| {
                users
                    .get(&line.authorizer)?
                    .verify(data.as_bytes(), &s)
                    .ok()
            })
            .ok_or(Error::Other("Invalid signature.".into()))?;

        std::mem::swap(&mut signature, &mut line.signature);
        self.check(&line)?;
        self.handle_unchecked(line);
        Ok(())
    }
    #[must_use]
    pub fn get_pos(&self) -> (u32, u32) {
        let mut rand = self.rand.clone();
        let mut x = 0;
        let mut y = 0;
        while self.board.contains_key(&(x, y)) {
            let random = rand.next_u64();
            #[allow(clippy::cast_possible_truncation)]
            let low: u32 = random as u32;
            #[allow(clippy::cast_possible_truncation)]
            let high: u32 = (random >> 32) as u32;
            x = low % self.settings.width;
            y = high % self.settings.height;
        }
        (x, y)
    }
    pub fn get_pos_mut(&mut self) -> (u32, u32) {
        let mut x = 0;
        let mut y = 0;
        while self.board.contains_key(&(x, y)) {
            let random = self.rand.next_u64();
            #[allow(clippy::cast_possible_truncation)]
            let low: u32 = random as u32;
            #[allow(clippy::cast_possible_truncation)]
            let high: u32 = (random >> 32) as u32;
            x = low % self.settings.width;
            y = high % self.settings.height;
        }
        (x, y)
    }
    /// Check if a `MoveLine` is valid
    /// # Errors
    /// If the `line` is not valid.
    pub fn check(&self, line: &MoveLine) -> Result<(), Error> {
        match line.move_type {
            MoveLineType::Join => {
                if self.players.contains_key(&line.authorizer) {
                    return Err(Error::Unautherized(line.authorizer));
                }
                let pos = self.get_pos();
                if line.x.ok_or(Error::MalformedMove)? != pos.0
                    || line.y.ok_or(Error::MalformedMove)? != pos.1
                {
                    Err(Error::OutOfRange(
                        "Position".into(),
                        format!("({}, {})", pos.0, pos.1),
                    ))
                } else {
                    Ok(())
                }
            }
            MoveLineType::Drive => {
                let x = line.x.ok_or(Error::MalformedMove)?;
                let y = line.y.ok_or(Error::MalformedMove)?;
                let player = self.get_player(line.authorizer)?;
                player.is_alive(true)?;
                player.has_points()?;
                if self.board.contains_key(&(x, y)) {
                    return Err(Error::NotFound("free tile".into()));
                }
                player.in_range(x, y, 1)?;
                Ok(())
            }
            MoveLineType::Shoot | MoveLineType::Gift => {
                let target = line.target.ok_or(Error::MalformedMove)?;
                let t = self.get_player(target)?;
                let p = self.get_player(line.authorizer)?;
                t.is_alive(true)?;
                p.is_alive(true)?;
                p.has_points()?;
                p.in_range(t.x, t.y, self.settings.range.get_range(p.level))?;
                Ok(())
            }
            MoveLineType::Vote => {
                let target = line.target.ok_or(Error::MalformedMove)?;
                let t = self.get_player(target)?;
                let p = self.get_player(line.authorizer)?;
                t.is_alive(true)?;
                p.is_alive(false)?;
                Ok(())
            }
            MoveLineType::HandleVotes => Ok(()),
            MoveLineType::Upgrade => {
                let p = self.get_player(line.authorizer)?;
                p.is_alive(true)?;
                p.upgradable(self.settings.max_level)?;
                p.has_points()?;
                Ok(())
            }
        }
    }
    fn handle_unchecked(&mut self, line: MoveLine) {
        match line.move_type {
            MoveLineType::Join => {
                let (x, y) = self.get_pos_mut();
                self.players.insert(
                    line.authorizer,
                    Player {
                        user: line.authorizer,
                        health: self.settings.health,
                        level: 0,
                        points: 1,
                        x,
                        y,
                    },
                );
                self.board.insert((x, y), line.authorizer);
            }
            MoveLineType::Drive => {
                let x = line.x.ok_or(Error::MalformedMove).unwrap();
                let y = line.y.ok_or(Error::MalformedMove).unwrap();
                let player = self.players.get_mut(&line.authorizer).unwrap();
                player.points -= 1;
                self.board.remove(&(player.x, player.y));
                player.x = x;
                player.y = y;
                self.board.insert((x, y), line.authorizer);
            }
            MoveLineType::Shoot => {
                let target = line.target.ok_or(Error::MalformedMove).unwrap();
                let target = self.players.get_mut(&target).unwrap();
                target.health -= 1;
                let add = (target.health == 0)
                    .then(|| std::mem::replace(&mut target.points, 0))
                    .unwrap_or(0);
                let player = self.players.get_mut(&line.authorizer).unwrap();
                player.points -= 1;
                player.points += add;
            }
            MoveLineType::Gift => {
                let target = line.target.unwrap();
                let player = self.players.get_mut(&line.authorizer).unwrap();
                player.points -= 1;
                let target = self.players.get_mut(&target).unwrap();
                target.points += 1;
            }
            MoveLineType::Vote => {
                let target = line.target.unwrap();
                self.votes.insert(line.authorizer, target);
            }
            MoveLineType::HandleVotes => {
                self.players.iter_mut().for_each(|(_, p)| p.points += 1);
                let mut votes = HashMap::new();
                std::mem::swap(&mut votes, &mut self.votes);
                for player in votes
                    .into_iter()
                    .fold(HashMap::<i32, u32>::new(), |mut x, (_, y)| {
                        *x.entry(y).or_default() += 1;
                        x
                    })
                    .into_iter()
                    .filter(|(_, v)| v >= &self.settings.vote_threshold)
                    .map(|(x, _)| x)
                {
                    if let Some(player) = self
                        .players
                        .get_mut(&player)
                        .and_then(|player| player.is_alive(true).is_ok().then_some(player))
                    {
                        player.points += 1;
                    }
                }
            }
            MoveLineType::Upgrade => {
                self.players.get_mut(&line.authorizer).unwrap().level += 1;
            }
        }
        self.lines.push(line);
    }
}
#[derive(Clone)]
pub struct Settings {
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    pub health: u32,
    pub max_level: i32,
    pub max_players: i32,
    pub vote_threshold: u32,
    pub range: LevelRangeMap,
}
impl Default for Settings {
    fn default() -> Self {
        Settings {
            seed: 0,
            width: 5,
            height: 5,
            max_level: 2,
            range: LevelRangeMap::Linear,
            health: 3,
            max_players: 10,
            vote_threshold: 3,
        }
    }
}
#[derive(Debug)]
pub enum Error {
    NotFound(String),           // the thing that wasn't found
    OutOfRange(String, String), // what (capitalized), range
    Unautherized(i32),          // the player
    MalformedMove,
    Other(String),
}
impl std::error::Error for Error {}
impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotFound(a) => write!(f, "Could not find {a}."),
            Error::OutOfRange(a, b) => write!(f, "{a} out of range: {b}."),
            Error::Unautherized(a) => write!(f, "Player ({a}) is unautherized."),
            Error::MalformedMove => write!(f, "Move was malformed."),
            Error::Other(a) => write!(f, "{a}"),
        }
    }
}
#[derive(Clone)]
pub enum LevelRangeMap {
    Linear,
    Array(Vec<u32>),
}
impl std::str::FromStr for LevelRangeMap {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text == "L" {
            Ok(LevelRangeMap::Linear)
        } else {
            text.starts_with('A')
                .then_some(())
                .ok_or(Error::NotFound("LevelRangeMap".into()))?;
            let mut parts = text.split('.');
            let first = parts.next().map(|x| &x[1..]);
            let parts = first
                .into_iter()
                .chain(parts)
                .map(str::parse)
                .try_fold(Vec::new(), |mut x, y| match y {
                    Ok(y) => {
                        x.push(y);
                        Some(x)
                    }
                    _ => None,
                })
                .ok_or(Error::MalformedMove)?;
            Ok(LevelRangeMap::Array(parts))
        }
    }
}
impl Display for LevelRangeMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LevelRangeMap::Linear => write!(f, "L"),
            LevelRangeMap::Array(a) => write!(
                f,
                "A{}|",
                a.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
            ),
        }
    }
}
impl LevelRangeMap {
    #[must_use]
    pub fn get_range(&self, level: u32) -> u32 {
        match self {
            LevelRangeMap::Linear => level + 1,
            LevelRangeMap::Array(a) => a[level as usize],
        }
    }
}

use axum::{
    async_trait,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        FromRequestParts, Path, Query, State, TypedHeader,
    },
    headers::Authorization,
    http::{header, request, HeaderValue, Method, Request, StatusCode},
    middleware::{self, from_extractor, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, get_service, post},
    Extension, Json, RequestPartsExt, Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use headers::authorization::Bearer;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    sync::{atomic::AtomicUsize, Arc, Mutex},
};
use tokio::sync::broadcast;
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
};

static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(0);

struct AppState {
    rooms: Mutex<HashMap<String, Room>>, // Keys are the name of the channel
    users: Mutex<HashMap<usize, User>>,
    tx: broadcast::Sender<String>,
}

#[derive(Clone, Serialize)]
struct User {
    id: usize,
    name: String,
}

#[derive(Clone, Serialize)]
struct Room {
    name: String, // name will be unique and act as id for now
    user_set: HashSet<usize>,
    messages: Vec<UserMessage>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UserMessage {
    user_id: usize,
    message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Claims {
    user_id: usize,
}

#[async_trait]
impl FromRequestParts<Arc<AppState>> for Claims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut request::Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED.into_response())?;

        let user_id: usize = bearer.token().parse().unwrap();
        let users = state.users.lock().unwrap();
        users
            .get(&user_id)
            .ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())?;

        Ok(Self { user_id })
    }
}

#[tokio::main]
async fn main() {
    let app_state = Arc::new(AppState {
        rooms: Mutex::new(HashMap::new()),
        users: Mutex::new(HashMap::new()),
        tx: broadcast::channel(100).0,
    });

    let authorized_routes: Router = Router::new()
        .route("/login", get(login))
        .route("/users", get(users))
        .route("/rooms", get(rooms))
        .route("/rooms/:room_name", post(create_room))
        .route("/rooms/:room_name/join", post(join_room))
        .route_layer(
            middleware::from_extractor_with_state::<Claims, Arc<AppState>>(app_state.clone()),
        )
        .with_state(app_state.clone());

    let serve_dir = ServeDir::new("dist").not_found_service(ServeFile::new("dist/index.html"));
    let public_routes = Router::new()
        .route("/register/:user_name", post(register))
        .route("/:user_id", get(websocket_handler))
        .with_state(app_state.clone());

    let app = Router::new()
        .nest_service("/chat", serve_dir.clone())
        .merge(authorized_routes)
        .merge(public_routes)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
                .allow_private_network(true),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], 3005));
    println!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn register(Path(user_name): Path<String>, State(state): State<Arc<AppState>>) -> Response {
    let mut users = state.users.lock().unwrap();
    let user_id = NEXT_USER_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    users.insert(
        user_id,
        User {
            id: user_id,
            name: user_name,
        },
    );
    Json(json!({ "user_id": user_id })).into_response()
}

async fn login(Claims { user_id }: Claims, State(state): State<Arc<AppState>>) -> Response {
    let users = state.users.lock().unwrap();
    match users.get(&user_id) {
        Some(user) => Json(user).into_response(),
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn create_room(
    Path(room_name): Path<String>,
    Claims { user_id }: Claims,
    State(state): State<Arc<AppState>>,
) -> Response {
    let mut rooms = state.rooms.lock().unwrap();
    if rooms.contains_key(&room_name) {
        return StatusCode::CONFLICT.into_response();
    }
    let room = Room {
        name: room_name.clone(),
        user_set: HashSet::from([user_id]),
        messages: Vec::new(),
    };
    rooms.insert(room_name, room);
    StatusCode::CREATED.into_response()
}

async fn rooms(State(state): State<Arc<AppState>>) -> Response {
    let rooms = state.rooms.lock().unwrap();
    Json(&(*rooms)).into_response()
}

async fn users(State(state): State<Arc<AppState>>) -> Response {
    let users = state.users.lock().unwrap();
    Json(&(*users)).into_response()
}

async fn join_room(
    Path(room_name): Path<String>,
    Claims { user_id }: Claims,
    State(state): State<Arc<AppState>>,
) -> Response {
    println!("join_room: {}", user_id);
    let mut rooms = state.rooms.lock().unwrap();

    match rooms.get_mut(&room_name) {
        Some(room) => {
            room.user_set.insert(user_id);
            // Send joined message to all subscribers.
            // let msg = format!(
            //     "join\n{}\n{}",
            //     room_name,
            //     state.users.lock().unwrap().get(&user_id).unwrap().name
            // );
            // let _ = state.tx.send(msg);
            Json(room).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<usize>,
) -> Response {
    match state.users.lock().unwrap().get(&user_id) {
        Some(user) => println!("websocket_handler: {}", user.id),
        None => return StatusCode::BAD_REQUEST.into_response(),
    }
    ws.on_upgrade(move |socket| websocket(socket, state, user_id))
}

async fn websocket(stream: WebSocket, state: Arc<AppState>, user_id: usize) {
    // let user_name = state
    //     .users
    //     .lock()
    //     .unwrap()
    //     .get(&user_id)
    //     .unwrap()
    //     .name
    //     .clone();

    let (mut sender, mut receiver) = stream.split();
    let tx = state.tx.clone();
    let mut rx = tx.subscribe();
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            // In any websocket error, break loop.
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });
    while let Some(Ok(message)) = receiver.next().await {
        if let Message::Text(msg) = message {
            if let Some((room_name, message)) = msg.split_once('\n') {
                state
                    .rooms
                    .lock()
                    .unwrap()
                    .get_mut(room_name)
                    .unwrap()
                    .messages
                    .push(UserMessage {
                        user_id,
                        message: message.into(),
                    });
                tx.send(format!("{}\n{}", user_id, msg)).unwrap();
            }
        }
    }
    send_task.abort();
}

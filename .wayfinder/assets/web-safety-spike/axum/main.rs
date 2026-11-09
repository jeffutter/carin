use axum::{routing::get, Router, response::IntoResponse};
async fn h() -> impl IntoResponse {
    ([("content-security-policy","default-src 'none'"),("x-content-type-options","nosniff")], "hi")
}
#[tokio::main]
async fn main() {
    let a = tokio::net::TcpListener::bind("127.0.0.1:8811").await.unwrap();
    let b = tokio::net::TcpListener::bind("127.0.0.1:8812").await.unwrap();
    let app = || Router::new().route("/", get(h));
    tokio::try_join!(axum::serve(a, app()), axum::serve(b, app())).unwrap();
}

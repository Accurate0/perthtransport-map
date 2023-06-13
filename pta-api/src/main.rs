use crate::{graphql::query::QueryRoot, routes::health::health_check};
use async_graphql::{extensions::Tracing, EmptyMutation, EmptySubscription, Schema};
use axum::{
    body::Body,
    http::{
        header::{ACCEPT_ENCODING, CONTENT_TYPE, USER_AGENT},
        HeaderMap, Request,
    },
    routing::get,
    Extension, Router,
};
use http::Method;
use reqwest::header::ACCEPT;
use reqwest_tracing::TracingMiddleware;
use std::{net::SocketAddr, sync::Arc};
use tower::limit::GlobalConcurrencyLimitLayer;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::Level;
use types::AppState;

mod graphql;
mod routes;
mod types;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    pta::log::init_logger();
    let config = pta::config::get_application_config()?;

    let redis = redis::Client::open(config.redis_connection_string.clone())?;
    let state = AppState { redis };

    let mut default_headers = HeaderMap::new();
    default_headers.append(ACCEPT_ENCODING, "gzip".parse()?);
    default_headers.append(CONTENT_TYPE, "application/json".parse()?);
    default_headers.append(ACCEPT, "application/json".parse()?);
    default_headers.append(USER_AGENT, "okhttp/4.9.2".parse()?);

    let http_client = Arc::new(
        reqwest_middleware::ClientBuilder::new(
            reqwest::ClientBuilder::new()
                .default_headers(default_headers)
                .build()?,
        )
        .with(TracingMiddleware::default())
        .build(),
    );

    let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .data(http_client)
        .data(config)
        .extension(Tracing)
        .finish();

    let routes = Router::new()
        .route(
            "/graphql",
            get(routes::graphql::graphiql).post(routes::graphql::graphql_handler),
        )
        .route("/ws", get(routes::websocket::handler))
        // maccas api is /health/status but this makes more sense
        .route("/status/health", get(health_check))
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list([
                    "http://localhost:3000".parse()?,
                    "https://perthtransport.xyz".parse()?,
                ]))
                .allow_methods([Method::GET, Method::POST]),
        )
        .layer(Extension(schema))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<Body>| {
                    tracing::info_span!("http-request", uri = request.uri().to_string())
                })
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                ),
        )
        .layer(GlobalConcurrencyLimitLayer::new(2048))
        .with_state(state);

    let app = Router::new().nest("/v1", routes);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::info!("server starting on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;

    Ok(())
}

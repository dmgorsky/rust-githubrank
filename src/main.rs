use std::io::IsTerminal;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{routing::get, Json, Router};
use serde_json::json;
use tracing::{error, info};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use gh_svc::GithubService;

#[derive(Clone)]
pub struct AppState {
    github_service: Arc<GithubService>,
}

#[tokio::main]
async fn main() {
    // tracing + logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gh_svc=debug,tower_http=debug".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(std::io::stderr().is_terminal())
                .with_writer(std::io::stdout /*stderr*/)
                .with_span_events(FmtSpan::CLOSE),
        )
        .init();

    //toplevel openapi
    #[derive(OpenApi)]
    #[openapi(
    paths(
    get_org_contributors,
    ),
    components(
    ),
    tags(
    (name = "githubrank", description = "Scalac challenge - in Rust")
    )
    )]
    struct ApiDoc;

    let token = std::env::var("GH_TOKEN");
    match token {
        Ok(_) => {
            info!("Using GH_TOKEN env variable");
        }
        Err(_) => {
            error!("GH_TOKEN env variable not found!");
        }
    };

    // 'shared state' with github service for all handlers
    let gh_svc = GithubService::new(token);
    let shared_github_service = Arc::new(gh_svc);
    let app_state = AppState {
        github_service: Arc::clone(&shared_github_service),
    };

    let app = Router::new()
        .route("/", get(root))
        .route("/org/:org_name/contributors", get(get_org_contributors))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .with_state(app_state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Listening to localhost:8080, use http://localhost:8080/swagger-ui to inspect");
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    "Githubrank http://localhost:8080/swagger-ui"
}

//handler openapi desc
#[utoipa::path(
get,
path = "/org/{org_name}/contributors",
responses(
(status = 200, description = "Contributors got w/o impediments",
body = (String, (String, Vec<String>)),
example = json!(
    {"result":
    [{"contributions": 356, "name": "<...>"}, {"contributions": 50, "name": "<...>"}]
    , "error": ""}
)),
(status = 418, description = "Unable to fetch", body = (String, (String, Vec<String>)), example = json!(
{"result": "", "error": "Unable to fetch {...} repositories"}
)),

),
params(
("org_name" = string, Path, description = "org name")
),
security(
("api_key" = [])
)
)]
async fn get_org_contributors(
    org_name: Path<String>,
    State(AppState { github_service, .. }): State<AppState>, //only needed fields
                                                             // State(app_state): State<AppState> // all contents
) -> impl IntoResponse {
    let r = github_service
        .get_repos_with_contributors_v2(org_name.0)
        .await;
    if r.is_ok() {
        // for IntoResponse we can return tuples with status code and result
        // https://docs.rs/axum/latest/axum/response/index.html
        (
            StatusCode::OK,
            Json(json!({"result": r.unwrap(), "error": ""})),
        )
    } else {
        (
            StatusCode::IM_A_TEAPOT,
            Json(json!({"result": "", "error": r.unwrap_err().to_string()})),
        )
    }
}

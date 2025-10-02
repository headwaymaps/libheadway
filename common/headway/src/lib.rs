use axum::{response::Html, routing::get, Router};
use tokio::runtime::Runtime;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum ServerError {
    #[error("Failed to create runtime: {0}")]
    RuntimeError(String),
    #[error("Failed to bind to {addr}: {error}")]
    BindError { addr: String, error: String },
    #[error("Server error: {0}")]
    ServeError(String),
}

/// Starts a local web server on the specified port with a hello_world endpoint.
/// Returns a handle that can be used to manage the server.
#[uniffi::export]
pub fn start_server(addr: &str) -> Result<(), ServerError> {
    // Create a new Tokio runtime
    let rt = Runtime::new().map_err(|e| ServerError::RuntimeError(e.to_string()))?;

    rt.block_on(async {
        // Build the router with a hello_world endpoint
        let app = Router::new()
            .route("/hello_world", get(hello_world));

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| ServerError::BindError {
                addr: addr.to_string(),
                error: e.to_string(),
            })?;

        println!("Server running on http://{}", addr);

        axum::serve(listener, app)
            .await
            .map_err(|e| ServerError::ServeError(e.to_string()))?;

        Ok::<(), ServerError>(())
    })
}

async fn hello_world() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

uniffi::setup_scaffolding!();

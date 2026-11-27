//! Files kept on a server that versions each one: the interface the editor
//! saves a page and its comment file through. osom-hub implements it for
//! the agent's space.
use std::future::Future;
use std::pin::Pin;

#[cfg(not(target_arch = "wasm32"))]
pub type FileFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
pub type FileFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

/// Files on a server that versions each one (an etag) and refuses a write
/// over a version it no longer has. Asynchronous on both native and wasm32.
pub trait PageFiles: Send + Sync {
    /// The file and its version, or `None` when there is no such file.
    fn read(&self, path: String) -> FileFuture<Result<Option<Versioned>, String>>;

    /// Write over version `etag` (`None`: whatever is there); the new version.
    fn write(&self, path: String, text: String, etag: Option<String>) -> FileFuture<Result<Option<String>, WriteError>>;

    /// Where `path` opens in a browser: the server's own web app. A link's
    /// card opens linked pages there.
    fn web_url(&self, path: &str) -> String;
}

/// A file's text and the version it was read at.
#[derive(Clone, Debug, PartialEq)]
pub struct Versioned {
    pub text: String,
    pub etag: Option<String>,
}

/// Why a write did not happen.
#[derive(Clone, Debug, PartialEq)]
pub enum WriteError {
    /// Someone changed the file since `etag`.
    Conflict,
    Failed(String),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict => f.write_str("The page changed on the server since it was opened. Reload it before saving."),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

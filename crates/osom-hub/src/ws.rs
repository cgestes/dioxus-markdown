//! Minimal platform socket + runtime shim for `osom-hub`.
//!
//! - Native: runs `reqwest` / `tokio-tungstenite` futures on a shared background
//!   Tokio runtime so callers (Dioxus winit loop, CLI, tests) never need an
//!   ambient Tokio reactor on the calling thread.
//! - Browser (`wasm32`): awaits `reqwest` directly on the JS event loop and
//!   wraps `web_sys::WebSocket` into the same `Socket` interface.
pub(crate) enum Next {
    Text(String),
    #[cfg(not(target_arch = "wasm32"))]
    Tick,
    Closed,
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::Next;
    use crate::Result;
    use futures_util::{SinkExt, StreamExt};
    use std::future::Future;
    use std::sync::OnceLock;
    use std::time::Duration;
    use tokio::net::TcpStream;
    use tokio::runtime::Runtime;
    use tokio_tungstenite::tungstenite::Message as Frame;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

    fn runtime() -> &'static Runtime {
        static RT: OnceLock<Runtime> = OnceLock::new();
        RT.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("osom-hub tokio runtime")
        })
    }

    /// Run a future on `osom-hub`'s background Tokio runtime and await its result
    /// from any executor (Dioxus main thread, tests, CLI).
    pub(crate) fn io<F>(fut: F) -> impl Future<Output = F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let handle = runtime().spawn(fut);
        async move { handle.await.expect("osom-hub I/O task") }
    }

    /// Block the current thread on an async future (used by native CLI & tests).
    pub fn block_on<F: Future>(fut: F) -> F::Output {
        runtime().block_on(fut)
    }

    pub(crate) struct Socket {
        ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    }

    impl Socket {
        pub(crate) async fn connect(url: &str, token: &str) -> Result<Self> {
            let mut request = url.into_client_request().map_err(|e| e.to_string())?;
            request.headers_mut().insert(
                "Sec-WebSocket-Protocol",
                HeaderValue::from_str(&format!("zeroclaw.v1, bearer.{token}")).map_err(|e| e.to_string())?,
            );
            let (ws, _) = tokio_tungstenite::connect_async(request)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Self { ws })
        }

        pub(crate) async fn send_text(&mut self, text: String) -> Result<()> {
            self.ws
                .send(Frame::Text(text.into()))
                .await
                .map_err(|e| e.to_string())
        }

        pub(crate) async fn next_text(&mut self) -> Result<Next> {
            loop {
                match tokio::time::timeout(Duration::from_secs(5), self.ws.next()).await {
                    Err(_) => return Ok(Next::Tick),
                    Ok(None) => return Ok(Next::Closed),
                    Ok(Some(Ok(Frame::Text(t)))) => return Ok(Next::Text(t.to_string())),
                    Ok(Some(Ok(Frame::Close(_)))) => return Ok(Next::Closed),
                    Ok(Some(Ok(_))) => continue,
                    Ok(Some(Err(e))) => return Err(e.to_string()),
                }
            }
        }

        pub(crate) async fn close(&mut self) {
            let _ = self.ws.close(None).await;
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::Next;
    use crate::Result;
    use futures_channel::mpsc::{UnboundedReceiver, unbounded};
    use futures_channel::oneshot;
    use futures_util::StreamExt;
    use std::future::Future;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

    pub(crate) fn io<F: Future>(fut: F) -> F {
        fut
    }

    fn js_err(v: JsValue) -> String {
        v.as_string()
            .or_else(|| js_sys::JSON::stringify(&v).ok().and_then(|s| s.as_string()))
            .unwrap_or_else(|| format!("{v:?}"))
    }

    pub(crate) struct Socket {
        ws: WebSocket,
        rx: UnboundedReceiver<Result<Next>>,
        _onopen: Closure<dyn FnMut()>,
        _onmessage: Closure<dyn FnMut(MessageEvent)>,
        _onerror: Closure<dyn FnMut(ErrorEvent)>,
        _onclose: Closure<dyn FnMut(CloseEvent)>,
    }

    impl Socket {
        pub(crate) async fn connect(url: &str, token: &str) -> Result<Self> {
            let protocols = js_sys::Array::new();
            protocols.push(&JsValue::from_str("zeroclaw.v1"));
            protocols.push(&JsValue::from_str(&format!("bearer.{token}")));
            let ws = WebSocket::new_with_str_sequence(url, &protocols).map_err(js_err)?;

            let (open_tx, open_rx) = oneshot::channel::<Result<()>>();
            let open_tx = std::rc::Rc::new(std::cell::RefCell::new(Some(open_tx)));
            let (tx, rx) = unbounded::<Result<Next>>();

            let onopen = {
                let open_tx = open_tx.clone();
                Closure::<dyn FnMut()>::new(move || {
                    if let Some(tx) = open_tx.borrow_mut().take() {
                        let _ = tx.send(Ok(()));
                    }
                })
            };
            let onmessage = {
                let tx = tx.clone();
                Closure::<dyn FnMut(MessageEvent)>::new(move |ev: MessageEvent| {
                    if let Some(text) = ev.data().as_string() {
                        let _ = tx.unbounded_send(Ok(Next::Text(text)));
                    }
                })
            };
            let onerror = {
                let open_tx = open_tx.clone();
                let tx = tx.clone();
                Closure::<dyn FnMut(ErrorEvent)>::new(move |_ev: ErrorEvent| {
                    if let Some(otx) = open_tx.borrow_mut().take() {
                        let _ = otx.send(Err("WebSocket connection error".into()));
                    } else {
                        let _ = tx.unbounded_send(Err("WebSocket connection error".into()));
                    }
                })
            };
            let onclose = {
                let open_tx = open_tx.clone();
                Closure::<dyn FnMut(CloseEvent)>::new(move |_ev: CloseEvent| {
                    if let Some(otx) = open_tx.borrow_mut().take() {
                        let _ = otx.send(Err("The agent closed the connection".into()));
                    }
                    let _ = tx.unbounded_send(Ok(Next::Closed));
                })
            };

            ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
            ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));

            open_rx
                .await
                .map_err(|_| "WebSocket connection cancelled".to_string())??;

            Ok(Self {
                ws,
                rx,
                _onopen: onopen,
                _onmessage: onmessage,
                _onerror: onerror,
                _onclose: onclose,
            })
        }

        pub(crate) async fn send_text(&mut self, text: String) -> Result<()> {
            self.ws.send_with_str(&text).map_err(js_err)
        }

        pub(crate) async fn next_text(&mut self) -> Result<Next> {
            match self.rx.next().await {
                Some(item) => item,
                None => Ok(Next::Closed),
            }
        }

        pub(crate) async fn close(&mut self) {
            self.ws.set_onopen(None);
            self.ws.set_onmessage(None);
            self.ws.set_onerror(None);
            self.ws.set_onclose(None);
            let _ = self.ws.close();
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            self.ws.set_onopen(None);
            self.ws.set_onmessage(None);
            self.ws.set_onerror(None);
            self.ws.set_onclose(None);
            let _ = self.ws.close();
        }
    }
}

pub(crate) use imp::{Socket, io};
#[cfg(not(target_arch = "wasm32"))]
pub use imp::block_on;

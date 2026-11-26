//! AppKit sends doCommandBySelector and winit also forwards the physical key.
//! Deliver an editor-owned command once. Unpaired selectors (including custom
//! Cocoa bindings) and marked-text input still go through the native renderer.
use blitz_dom::BaseDocument;
use blitz_shell::{BlitzShellProxy, WindowConfig, create_default_event_loop};
use dioxus_native::{
    prelude::Element,
    DioxusDocument, DioxusNativeApplication, DioxusNativeWindowRenderer, DocumentConfig,
    WindowAttributes,
    prelude::VirtualDom,
    winit::{
        application::ApplicationHandler,
        event::{ElementState, StartCause, WindowEvent},
        event_loop::ActiveEventLoop,
        keyboard::{Key, NamedKey},
        platform::macos::ApplicationHandlerExtMacOS,
        window::WindowId,
    },
};
use std::{cell::RefCell, rc::Rc};

struct EditorWindow {
    app: DioxusNativeApplication,
    document: Rc<RefCell<BaseDocument>>,
    pending: Option<(WindowId, String, NamedKey)>,
}
impl EditorWindow {
    fn flush(&mut self, events: &dyn ActiveEventLoop) {
        if let Some((window, action, _)) = self.pending.take()
            && let Some(native) = self.app.macos_handler()
        {
            native.standard_key_binding(events, window, &action);
        }
    }
}
impl ApplicationHandlerExtMacOS for EditorWindow {
    fn standard_key_binding(
        &mut self,
        events: &dyn ActiveEventLoop,
        window: WindowId,
        action: &str,
    ) {
        self.flush(events);
        let doc = self.document.borrow();
        let node = doc.get_focussed_node_id().and_then(|id| doc.get_node(id));
        let input = node
            .and_then(|n| n.element_data()?.text_input_data())
            .is_some();
        let cells = node.is_some_and(|n| n.attr("data-range-keys".into()) == Some("true"));
        let in_grid = node
            .and_then(|n| n.attr("id".into()))
            .is_some_and(|id| id.starts_with("cell-"));
        drop(doc);
        let key = match action {
            "insertNewline:" => Some(NamedKey::Enter),
            "moveUp:" | "moveUpAndModifySelection:" => Some(NamedKey::ArrowUp),
            "moveDown:" | "moveDownAndModifySelection:" => Some(NamedKey::ArrowDown),
            "moveLeft:" | "moveLeftAndModifySelection:" => Some(NamedKey::ArrowLeft),
            "moveRight:" | "moveRightAndModifySelection:" => Some(NamedKey::ArrowRight),
            "moveWordLeftAndModifySelection:" if in_grid => Some(NamedKey::ArrowLeft),
            "moveWordRightAndModifySelection:" if in_grid => Some(NamedKey::ArrowRight),
            "deleteForward:" => Some(NamedKey::Delete),
            "deleteBackward:" if cells => Some(NamedKey::Backspace),
            _ => None,
        };
        if input && let Some(key) = key {
            self.pending = Some((window, action.to_owned(), key));
        } else if let Some(native) = self.app.macos_handler() {
            native.standard_key_binding(events, window, action);
        }
    }
}
impl ApplicationHandler for EditorWindow {
    fn macos_handler(&mut self) -> Option<&mut dyn ApplicationHandlerExtMacOS> {
        Some(self)
    }
    fn window_event(&mut self, events: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let WindowEvent::KeyboardInput { event: key, .. } = &event {
            let paired = self.pending.as_ref().is_some_and(|(window, _, expected)| {
                *window == id
                    && key.state == ElementState::Pressed
                    && key.logical_key == Key::Named(*expected)
            });
            if paired {
                self.pending = None;
            } else {
                self.flush(events);
            }
        }
        self.app.window_event(events, id, event);
    }
    fn about_to_wait(&mut self, events: &dyn ActiveEventLoop) {
        self.flush(events);
        self.app.about_to_wait(events);
    }
    fn new_events(&mut self, events: &dyn ActiveEventLoop, cause: StartCause) {
        self.app.new_events(events, cause);
    }
    fn can_create_surfaces(&mut self, events: &dyn ActiveEventLoop) {
        self.app.can_create_surfaces(events);
    }
    fn destroy_surfaces(&mut self, events: &dyn ActiveEventLoop) {
        self.app.destroy_surfaces(events);
    }
    fn resumed(&mut self, events: &dyn ActiveEventLoop) {
        self.app.resumed(events);
    }
    fn suspended(&mut self, events: &dyn ActiveEventLoop) {
        self.app.suspended(events);
    }
    fn proxy_wake_up(&mut self, events: &dyn ActiveEventLoop) {
        self.app.proxy_wake_up(events);
    }
}

pub(crate) fn launch(window: WindowAttributes, root: fn() -> Element) {
    let events = create_default_event_loop();
    let (proxy, queue) = BlitzShellProxy::new(events.create_proxy());
    #[cfg(debug_assertions)]
    {
        let proxy = proxy.clone();
        dioxus_devtools::connect(move |event| {
            proxy.send_event(blitz_shell::BlitzShellEvent::embedder_event(
                dioxus_native::DioxusNativeEvent::DevserverEvent(event),
            ))
        });
    }
    let doc = DioxusDocument::new(
        VirtualDom::new(root),
        DocumentConfig {
            font_ctx: Some(margin_ui::typefaces::fonts()),
            ..Default::default()
        },
    );
    let document = Rc::clone(&doc.inner);
    let config =
        WindowConfig::with_attributes(Box::new(doc), DioxusNativeWindowRenderer::default(), window);
    let app = DioxusNativeApplication::new(proxy, queue, config);
    events
        .run_app(EditorWindow {
            app,
            document,
            pending: None,
        })
        .expect("native event loop");
}

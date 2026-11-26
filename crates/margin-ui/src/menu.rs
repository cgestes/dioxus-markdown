//! Floating surfaces and the controls inside them: popovers, menus, toolbars.
use super::{Button, Icon, Variant};
use crate::prelude::*;

/// A floating surface (a menu, a picker). The caller positions it with
/// `class`/`style`. `keep_focus` stops a press from taking focus away from
/// the text being edited; `contain` keeps it from reaching the page below.
#[component]
pub fn Popover(
    #[props(default)] id: Option<String>,
    #[props(default)] class: String,
    #[props(default)] style: Option<String>,
    #[props(default)] role: Option<String>,
    #[props(default)] label: Option<String>,
    #[props(default = true)] keep_focus: bool,
    #[props(default = true)] contain: bool,
    #[props(default)] onwheel: Option<EventHandler<WheelEvent>>,
    children: Element,
) -> Element {
    rsx! {
        div { id, class: "ui-popover {class}", style, role, aria_label: label,
            onpointerdown: move |e| {
                if keep_focus {
                    e.prevent_default();
                }
                if contain {
                    e.stop_propagation();
                }
            },
            onwheel: move |e| {
                if let Some(h) = onwheel {
                    h.call(e);
                }
            },
            {children}
        }
    }
}

/// A list of actions or choices on a popover. `role` is `menu` (actions) or
/// `listbox` (a choice that follows typing, like the slash menu).
#[component]
pub fn Menu(
    #[props(default)] id: Option<String>,
    #[props(default)] class: String,
    #[props(default)] style: Option<String>,
    #[props(default)] role: Option<String>,
    #[props(default)] label: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        Popover { id, class: "ui-menu {class}", style, role: role.or(Some("menu".into())), label, {children} }
    }
}

/// One entry of a menu. `active` is the keyboard highlight; `selected` a
/// current choice; `danger` a destructive action. `hint` shows a shortcut.
#[component]
pub fn MenuItem(
    label: String,
    #[props(default)] id: Option<String>,
    #[props(default)] icon: Option<Icon>,
    /// A short mark when there is no icon: `H1`, `↑`.
    #[props(default)] badge: Option<String>,
    #[props(default)] hint: Option<String>,
    #[props(default)] active: bool,
    #[props(default)] selected: bool,
    #[props(default)] danger: bool,
    #[props(default)] disabled: bool,
    /// `option` inside a listbox; `menuitem` otherwise.
    #[props(default)] option: bool,
    #[props(default)] onhover: Option<EventHandler<()>>,
    onclick: EventHandler<()>,
) -> Element {
    let role = if option { "option" } else { "menuitem" };
    let class = if active { "ui-menu-item active" } else { "ui-menu-item" };
    rsx! {
        Button { label: label.clone(), text: label, icon, badge: badge.or_else(|| icon.is_none().then(String::new)), hint, id,
            variant: Variant::Bare, class, role: role.to_string(), danger, disabled,
            selected: (option || selected).then_some(selected || active), onhover, onclick }
    }
}

/// A small heading inside a menu.
#[component]
pub fn MenuSection(label: String) -> Element {
    rsx! { div { class: "ui-menu-section", "{label}" } }
}

/// A line between groups of menu items.
#[component]
pub fn MenuSeparator() -> Element {
    rsx! { div { class: "ui-menu-sep", role: "separator" } }
}

/// A row of icon buttons inside a menu (alignment, quick toggles).
#[component]
pub fn MenuRow(children: Element) -> Element {
    rsx! { div { class: "ui-menu-row", role: "group", {children} } }
}

/// How a toolbar sits on the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolbarLook {
    /// The light bar at the top of the editor.
    #[default]
    Dock,
    /// Dark and floating over the text: the format bar.
    Floating,
    /// Quiet and inline next to the caret: the empty-line tools.
    Inline,
}

impl ToolbarLook {
    fn class(self) -> &'static str {
        match self {
            Self::Dock => "toolbar-dock",
            Self::Floating => "toolbar-floating",
            Self::Inline => "toolbar-inline",
        }
    }
}

/// A row of buttons acting on what is selected or under the caret.
#[component]
pub fn Toolbar(
    label: String,
    #[props(default)] id: Option<String>,
    #[props(default)] class: String,
    #[props(default)] style: Option<String>,
    #[props(default)] look: ToolbarLook,
    #[props(default)] keep_focus: bool,
    #[props(default)] contain: bool,
    children: Element,
) -> Element {
    rsx! {
        div { id, class: "ui-toolbar {look.class()} {class}", role: "toolbar", aria_label: "{label}", style,
            onpointerdown: move |e| {
                if keep_focus {
                    e.prevent_default();
                }
                if contain {
                    e.stop_propagation();
                }
            },
            {children}
        }
    }
}

/// A divider between groups of toolbar buttons.
#[component]
pub fn ToolbarSep() -> Element {
    rsx! { span { class: "ui-toolbar-sep", aria_hidden: "true" } }
}

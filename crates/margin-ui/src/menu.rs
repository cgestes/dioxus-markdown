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
        div { id, class: "ui-popover flex flex-col p-1 border border-solid border-line rounded-lg bg-surface shadow-pop pointer-events-auto {class}", style, role, aria_label: label,
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
        Popover { id, class: "gap-[1px] min-w-[150px] {class}", style, role: role.or(Some("menu".into())), label, {children} }
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
    let base = "ui-menu-item flex items-center justify-start gap-2 flex-none w-full h-7 m-0 px-2 border-0 rounded-[5px] bg-transparent text-ink type-ui text-left whitespace-nowrap cursor-pointer select-none [&_.tool-caption]:flex-auto [&_.tool-caption]:text-left [&_.tool-icon]:flex-[0_0_16px] [&_.tool-icon]:w-4 [&_.tool-icon]:text-muted hover:bg-pale hover:text-accent-deep [&.active]:bg-pale [&.active]:text-accent-deep focus-visible:bg-pale focus-visible:text-accent-deep focus-visible:outline-none [&:hover_.tool-icon]:text-accent-deep [&.active_.tool-icon]:text-accent-deep [&:hover_.ui-badge-glyph]:text-accent-deep [&.active_.ui-badge-glyph]:text-accent-deep [&.danger:hover]:bg-alert-wash [&.danger:hover]:text-alert [&.danger:focus-visible]:bg-alert-wash [&.danger:focus-visible]:text-alert [&.danger:hover_.ui-badge-glyph]:text-alert disabled:opacity-38 disabled:bg-transparent disabled:text-ink disabled:cursor-default disabled:hover:bg-transparent disabled:hover:text-ink";
    let class = if active { format!("{base} active") } else { base.to_owned() };
    rsx! {
        Button { label: label.clone(), text: label, icon, badge: badge.or_else(|| icon.is_none().then(String::new)), hint, id,
            variant: Variant::Bare, class, role: role.to_string(), danger, disabled,
            selected: (option || selected).then_some(selected || active), onhover, onclick }
    }
}

/// A small heading inside a menu.
#[component]
pub fn MenuSection(label: String) -> Element {
    rsx! { div { class: "pt-1.5 px-2 pb-0.5 text-faint type-label uppercase", "{label}" } }
}

/// A line between groups of menu items.
#[component]
pub fn MenuSeparator() -> Element {
    rsx! { div { class: "flex-none h-[1px] my-[3px] mx-0.5 bg-line", role: "separator" } }
}

/// A row of icon buttons inside a menu (alignment, quick toggles).
#[component]
pub fn MenuRow(children: Element) -> Element {
    rsx! {
        div {
            class: "flex gap-0.5 p-[1px] [&_.ui-btn.icon]:flex-1 [&_.ui-btn.icon]:w-auto [&_.ui-btn.icon]:h-[26px] [&_.ui-btn.icon_.tool-icon]:w-[13px] [&_.ui-btn.icon_.tool-icon]:h-[13px] [&_.ui-btn.icon_.tool-icon_svg]:w-[13px] [&_.ui-btn.icon_.tool-icon_svg]:h-[13px]",
            role: "group",
            {children}
        }
    }
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
            Self::Dock => "",
            Self::Floating => "h-[39px] py-[3px] px-[5px] border border-solid border-toolbar-edge rounded-lg bg-toolbar shadow-bar [&_.ui-btn]:border-transparent [&_.ui-btn]:bg-transparent [&_.ui-btn]:text-toolbar-ink [&_.ui-btn.icon]:w-[34px] [&_.ui-btn.icon]:h-[30px] [&_.ui-btn:hover]:bg-toolbar-hover [&_.ui-btn:hover]:text-on-accent [&_.ui-btn:focus-visible]:bg-toolbar-hover [&_.ui-btn:focus-visible]:text-on-accent [&_.ui-btn.selected]:bg-toolbar-hover [&_.ui-btn.selected]:text-on-accent [&_.ui-toolbar-sep]:h-[18px] [&_.ui-toolbar-sep]:bg-white/20",
            Self::Inline => "min-h-6 py-0 px-[3px] rounded-md bg-page [&_.ui-btn.icon]:w-6 [&_.ui-btn.icon]:h-[22px] [&_.ui-btn.icon]:rounded-sm [&_.ui-btn.icon]:text-faint [&_.ui-btn.icon]:opacity-65 [&_.ui-btn.icon_.tool-icon]:w-[13px] [&_.ui-btn.icon_.tool-icon]:h-[13px] [&_.ui-btn.icon_.tool-icon_svg]:w-[13px] [&_.ui-btn.icon_.tool-icon_svg]:h-[13px] [&_.ui-btn.icon_.tool-caption]:type-label [&_.ui-btn.icon:hover]:opacity-100 [&_.ui-btn.icon:hover]:bg-pale [&_.ui-btn.icon:hover]:text-accent-deep [&_.ui-btn.icon:focus-visible]:opacity-100 [&_.ui-btn.icon:focus-visible]:bg-pale [&_.ui-btn.icon:focus-visible]:text-accent-deep [&_.ui-btn.ghost]:h-[22px] [&_.ui-btn.ghost]:px-[7px] [&_.ui-btn.ghost]:rounded-sm [&_.ui-btn.ghost]:type-hint",
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
        div { id, class: "ui-toolbar flex items-center gap-0.5 select-none {look.class()} {class}", role: "toolbar", aria_label: "{label}", style,
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
    rsx! { span { class: "ui-toolbar-sep flex-none w-[1px] h-[14px] mx-[3px] bg-line", aria_hidden: "true" } }
}

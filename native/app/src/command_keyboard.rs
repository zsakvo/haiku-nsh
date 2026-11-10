//! App-owned dismissal policy around the native Command's keyboard handling.
use nsh_core::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Props)]
pub struct CommandKeyboardProps {
    /// The containing palette is open.
    pub open: Prop<bool>,
    /// The native Command; its navigation and selection keys remain unchanged.
    pub children: Prop<Children>,
}

#[derive(Clone, Debug, PartialEq, Event)]
pub enum CommandKeyboardEvent {
    /// Escape requests closing the palette, including with a nonempty query.
    Cancel,
}

/// Gives the palette one-press Escape dismissal while its input has focus.
#[component]
pub fn CommandKeyboard(
    props: CommandKeyboardProps,
    cx: Scope<CommandKeyboardEvent>,
) -> impl IntoElement {
    let root = element_ref();
    let focused = signal(false);
    let armed = signal(false);
    root.on_focus_in(move || {
        focused.set(true);
        // Command installs its native keys when focus arrives. Bind after that
        // commit so dismissal wins over its default "clear, then cancel".
        after_answer(move || {
            if focused.get_untracked() {
                armed.set(true);
            }
        });
    });
    root.on_focus_out(move || {
        focused.set(false);
        armed.set(false);
    });
    effect(move || {
        if props.open.get() && armed.get() {
            on_key_with(
                "escape",
                KeyOptions {
                    when: nsh_core::keymap::When::Always,
                    ..KeyOptions::default()
                },
                move |_| cx.emit(CommandKeyboardEvent::Cancel),
            );
        }
    });
    view().element_ref(&root).child(props.children)
}

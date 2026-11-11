//! Global desktop shortcut registration. Carbon owns conflict detection on macOS.
use serde::{Deserialize, Serialize};
#[derive(Clone, Deserialize, Serialize)]
pub struct Binding {
    pub id: String,
    pub key_code: u32,
    pub modifiers: Vec<String>,
}
fn flags(binding: &Binding) -> Result<u32, String> {
    if binding.id.trim().is_empty() {
        return Err("Shortcut id is required".into());
    }
    if binding.key_code > 127 {
        return Err("Invalid macOS virtual key code".into());
    }
    let mut flags = 0;
    for modifier in &binding.modifiers {
        flags |= match modifier.as_str() {
            "cmd" | "meta" => 0x100,
            "shift" => 0x200,
            "alt" | "option" => 0x800,
            "ctrl" | "control" => 0x1000,
            _ => return Err(format!("Unknown modifier: {modifier}")),
        };
    }
    // Modifier-free and Shift-only shortcuts must never swallow normal typing globally.
    let function_key = [
        122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80, 90,
    ]
    .contains(&binding.key_code);
    if flags & (0x100 | 0x800 | 0x1000) == 0 && !function_key {
        return Err("Global shortcuts require Cmd, Ctrl, Option or a function key".into());
    }
    Ok(flags)
}
#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::{ffi::c_void, sync::Mutex};
    type Ref = *mut c_void;
    #[repr(C)]
    struct EventType {
        class: u32,
        kind: u32,
    }
    #[repr(C)]
    struct KeyId {
        signature: u32,
        id: u32,
    }
    struct Registered {
        binding: Binding,
        reference: usize,
    }
    static BINDINGS: Mutex<Vec<Registered>> = Mutex::new(Vec::new());
    static HANDLER: Mutex<Option<usize>> = Mutex::new(None);
    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn GetApplicationEventTarget() -> Ref;
        fn InstallEventHandler(
            target: Ref,
            handler: extern "C" fn(Ref, Ref, Ref) -> i32,
            count: usize,
            events: *const EventType,
            data: Ref,
            out: *mut Ref,
        ) -> i32;
        fn RegisterEventHotKey(
            code: u32,
            mods: u32,
            id: KeyId,
            target: Ref,
            options: u32,
            out: *mut Ref,
        ) -> i32;
        fn UnregisterEventHotKey(reference: Ref) -> i32;
        fn GetEventParameter(
            event: Ref,
            name: u32,
            kind: u32,
            actual_type: *mut u32,
            size: usize,
            actual_size: *mut usize,
            out: Ref,
        ) -> i32;
    }
    extern "C" fn handler(_next: Ref, event: Ref, _data: Ref) -> i32 {
        let mut key = KeyId {
            signature: 0,
            id: 0,
        };
        let status = unsafe {
            GetEventParameter(
                event,
                u32::from_be_bytes(*b"----"),
                u32::from_be_bytes(*b"hkid"),
                std::ptr::null_mut(),
                std::mem::size_of::<KeyId>(),
                std::ptr::null_mut(),
                &mut key as *mut _ as Ref,
            )
        };
        if status != 0 {
            return status;
        }
        if key.signature != u32::from_be_bytes(*b"HKUI") {
            return -9874;
        }
        if let Some(item) = BINDINGS.lock().unwrap().get(key.id as usize) {
            let _ =
                crate::events::emit("shortcuts.event", serde_json::json!({"id":item.binding.id}));
        }
        0
    }
    unsafe fn register(bindings: &[Binding]) -> Result<Vec<Registered>, String> {
        let mut registered: Vec<Registered> = Vec::new();
        for (index, binding) in bindings.iter().enumerate() {
            let mut reference = std::ptr::null_mut();
            let status = RegisterEventHotKey(
                binding.key_code,
                flags(binding)?,
                KeyId {
                    signature: u32::from_be_bytes(*b"HKUI"),
                    id: index as u32,
                },
                GetApplicationEventTarget(),
                0,
                &mut reference,
            );
            if status != 0 {
                for item in registered {
                    UnregisterEventHotKey(item.reference as Ref);
                }
                return Err(format!(
                    "Shortcut {} conflicts or cannot register (OSStatus {})",
                    binding.id, status
                ));
            }
            registered.push(Registered {
                binding: binding.clone(),
                reference: reference as usize,
            });
        }
        Ok(registered)
    }
    pub fn set(bindings: Vec<Binding>) -> Result<usize, String> {
        dispatch::Queue::main().exec_sync(move || unsafe {
            let mut installed = HANDLER.lock().unwrap();
            if installed.is_none() {
                let mut reference = std::ptr::null_mut();
                let event = EventType {
                    class: u32::from_be_bytes(*b"keyb"),
                    kind: 5,
                };
                let status = InstallEventHandler(
                    GetApplicationEventTarget(),
                    handler,
                    1,
                    &event,
                    std::ptr::null_mut(),
                    &mut reference,
                );
                if status != 0 {
                    return Err(format!("Cannot install shortcut handler: {status}"));
                }
                *installed = Some(reference as usize);
            }
            let mut current = BINDINGS.lock().unwrap();
            let previous: Vec<Binding> = current.iter().map(|x| x.binding.clone()).collect();
            replace_group(&previous, &bindings, |group| {
                for item in current.drain(..) {
                    UnregisterEventHotKey(item.reference as Ref);
                }
                *current = register(group)?;
                Ok(())
            })?;
            Ok(current.len())
        })
    }
}
pub fn set(bindings: Vec<Binding>) -> Result<usize, String> {
    let mut keys = std::collections::HashSet::new();
    let mut ids = std::collections::HashSet::new();
    for binding in &bindings {
        let mods = flags(binding)?;
        if !keys.insert((binding.key_code, mods)) {
            return Err("Duplicate global shortcut combination".into());
        }
        if !ids.insert(binding.id.clone()) {
            return Err("Duplicate shortcut id".into());
        }
    }
    #[cfg(target_os = "macos")]
    {
        mac::set(bindings)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = bindings;
        Ok(0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_typing_keys() {
        assert!(
            flags(&Binding {
                id: "play".into(),
                key_code: 35,
                modifiers: vec![]
            })
            .is_err()
        );
        assert!(
            flags(&Binding {
                id: "play".into(),
                key_code: 35,
                modifiers: vec!["shift".into()]
            })
            .is_err()
        );
    }
    #[test]
    fn permits_explicit_modifier() {
        assert_eq!(
            flags(&Binding {
                id: "play".into(),
                key_code: 35,
                modifiers: vec!["cmd".into(), "shift".into()]
            })
            .unwrap(),
            0x300
        );
    }
}

/// Validated, all-or-nothing registration boundary. Other platforms keep a no-op backend.
pub trait GlobalShortcuts: Send + Sync {
    fn supported(&self) -> bool;
    fn replace(&self, bindings: &[haiku_store::shortcuts::Binding]) -> Result<(), String>;
}
/// Carbon registrations in the native host; fixtures skip system registration explicitly.
pub struct SystemShortcuts {
    pub native: bool,
}
impl GlobalShortcuts for SystemShortcuts {
    fn supported(&self) -> bool {
        cfg!(target_os = "macos")
    }
    fn replace(&self, bindings: &[haiku_store::shortcuts::Binding]) -> Result<(), String> {
        let bindings = haiku_store::shortcuts::validate(bindings)?;
        if !self.native || !self.supported() {
            return Ok(());
        }
        let group = bindings
            .into_iter()
            .filter(|b| b.scope == "global" && !b.chord.is_empty())
            .map(|b| {
                let parts: Vec<_> = b.chord.split('+').collect();
                Binding {
                    id: b.id,
                    key_code: haiku_store::shortcuts::key_code(parts.last().unwrap()).unwrap(),
                    modifiers: parts[..parts.len() - 1]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                }
            })
            .collect();
        set(group).map(|_| ())
    }
}
/// Both the Carbon backend and fake backend exercise this replacement/rollback protocol.
#[cfg(any(target_os = "macos", test))]
fn replace_group<T>(
    previous: &[T],
    next: &[T],
    mut install: impl FnMut(&[T]) -> Result<(), String>,
) -> Result<(), String> {
    if let Err(error) = install(next) {
        install(previous).map_err(|restore| format!("{error}; 恢复先前快捷键失败：{restore}"))?;
        return Err(error);
    }
    Ok(())
}
#[cfg(test)]
mod transaction_tests {
    use super::*;
    #[test]
    fn rejected_system_registration_restores_entire_previous_group() {
        let mut live = vec![1, 2];
        let previous = live.clone();
        let mut attempts = Vec::new();
        let result = replace_group(&previous, &[3, 9, 4], |group| {
            attempts.push(group.to_vec());
            live.clear();
            for id in group {
                if *id == 9 {
                    live.clear();
                    return Err("system rejected 9".into());
                }
                live.push(*id);
            }
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(live, previous);
        assert_eq!(attempts, vec![vec![3, 9, 4], vec![1, 2]]);
        replace_group(&previous, &[5, 6], |group| {
            live = group.to_vec();
            Ok(())
        })
        .unwrap();
        assert_eq!(live, vec![5, 6]);
    }
    #[test]
    fn all_function_keys_are_valid_carbon_bindings() {
        for n in 1..=20 {
            assert!(
                flags(&Binding {
                    id: "play".into(),
                    key_code: haiku_store::shortcuts::key_code(&format!("f{n}")).unwrap(),
                    modifiers: vec![]
                })
                .is_ok()
            );
        }
    }
}

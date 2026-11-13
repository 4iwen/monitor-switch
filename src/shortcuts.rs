use global_hotkey::{
    hotkey::{HotKey, Modifiers},
    GlobalHotKeyManager,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShortcutBinding {
    pub display: String,
    // None means switch to the previous input on this display.
    pub value: Option<u16>,
    pub shortcut: String,
}

pub fn label(shortcut: &str) -> String {
    shortcut
        .replace("Control", "Ctrl")
        .replace(
            "Super",
            if cfg!(target_os = "macos") {
                "Cmd"
            } else {
                "Win"
            },
        )
        .replace("Key", "")
        .replace("Digit", "")
}

pub fn validate(
    bindings: &[ShortcutBinding],
) -> Result<HashMap<u32, (HotKey, ShortcutBinding)>, String> {
    let mut parsed = HashMap::new();
    let mut targets = std::collections::HashSet::new();
    for binding in bindings {
        let hotkey: HotKey = binding
            .shortcut
            .parse()
            .map_err(|error| format!("Shortcut {} is invalid: {error}", binding.shortcut))?;
        if !hotkey
            .mods
            .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
        {
            return Err("Shortcut must include a Control, Alt/Option or Command/Win modifier".into());
        }
        if !targets.insert((binding.display.clone(), binding.value)) {
            return Err("Each input can only have one shortcut".into());
        }
        if parsed
            .insert(hotkey.id(), (hotkey, binding.clone()))
            .is_some()
        {
            return Err(format!("Shortcut {} is duplicated, choose a different combination", binding.shortcut));
        }
    }
    Ok(parsed)
}

pub struct Shortcuts {
    manager: GlobalHotKeyManager,
    registered: HashMap<u32, (HotKey, ShortcutBinding)>,
}

impl Shortcuts {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            manager: GlobalHotKeyManager::new().map_err(|e| e.to_string())?,
            registered: HashMap::new(),
        })
    }

    pub fn restore_available(&mut self, bindings: &[ShortcutBinding]) -> Result<(), String> {
        restore_with(
            &mut self.registered,
            bindings,
            |hotkey| {
                self.manager
                    .register(hotkey)
                    .map_err(|error| error.to_string())
            },
            |hotkey| {
                self.manager
                    .unregister(hotkey)
                    .map_err(|error| error.to_string())
            },
        )
    }

    pub fn action(&self, id: u32) -> Option<ShortcutBinding> {
        self.registered.get(&id).map(|(_, binding)| binding.clone())
    }

    pub fn replace(&mut self, bindings: &[ShortcutBinding]) -> Result<(), String> {
        let next = validate(bindings)?;
        let mut added = Vec::new();
        for (id, (hotkey, binding)) in &next {
            if self.registered.contains_key(id) {
                continue;
            }
            if let Err(error) = self.manager.register(*hotkey) {
                for key in added {
                    let _ = self.manager.unregister(key);
                }
                return Err(format!(
                    "Cannot register {}, it may already be in use: {error}",
                    binding.shortcut
                ));
            }
            added.push(*hotkey);
        }
        for (id, (hotkey, _)) in &self.registered {
            if !next.contains_key(id) {
                let _ = self.manager.unregister(*hotkey);
            }
        }
        self.registered = next;
        Ok(())
    }
}

fn restore_with(
    registered: &mut HashMap<u32, (HotKey, ShortcutBinding)>,
    bindings: &[ShortcutBinding],
    mut register: impl FnMut(HotKey) -> Result<(), String>,
    mut unregister: impl FnMut(HotKey) -> Result<(), String>,
) -> Result<(), String> {
    let next = validate(bindings)?;
    let mut failures = Vec::new();
    for (id, (hotkey, binding)) in &next {
        if let Some(existing) = registered.get_mut(id) {
            existing.1 = binding.clone();
            continue;
        }
        match register(*hotkey) {
            Ok(()) => {
                registered.insert(*id, (*hotkey, binding.clone()));
            }
            Err(error) => failures.push(format!("{}：{error}", label(&binding.shortcut))),
        }
    }
    let removed = registered
        .keys()
        .filter(|id| !next.contains_key(id))
        .copied()
        .collect::<Vec<_>>();
    for id in removed {
        match unregister(registered[&id].0) {
            Ok(()) => {
                registered.remove(&id);
            }
            Err(error) => failures.push(error),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Some shortcuts are unavailable, others still work: {}",
            failures.join("；")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_keeps_available_keys_and_can_retry_conflicts() {
        let bindings = [
            ShortcutBinding {
                display: "a".into(),
                value: Some(17),
                shortcut: "Control+Alt+Digit1".into(),
            },
            ShortcutBinding {
                display: "a".into(),
                value: Some(18),
                shortcut: "Control+Alt+Digit2".into(),
            },
        ];
        let blocked = bindings[0].shortcut.parse::<HotKey>().unwrap().id();
        let working = bindings[1].shortcut.parse::<HotKey>().unwrap().id();
        let mut registered = HashMap::new();
        let result = restore_with(
            &mut registered,
            &bindings,
            |hotkey| {
                if hotkey.id() == blocked {
                    Err("occupied".into())
                } else {
                    Ok(())
                }
            },
            |_| panic!("must not roll back working shortcuts"),
        );
        assert!(result.unwrap_err().contains("Ctrl+Alt+1"));
        assert!(registered.contains_key(&working));
        assert!(!registered.contains_key(&blocked));
        restore_with(&mut registered, &bindings, |_| Ok(()), |_| Ok(())).unwrap();
        assert_eq!(registered.len(), 2);
    }
    #[test]
    fn rejects_duplicate_combinations_and_plain_keys() {
        let binding = ShortcutBinding {
            display: "a".into(),
            value: Some(17),
            shortcut: "Control+Alt+Digit1".into(),
        };
        assert!(validate(std::slice::from_ref(&binding)).is_ok());
        let mut other = binding.clone();
        other.display = "b".into();
        assert!(validate(&[binding, other.clone()]).is_err());
        other.shortcut = "KeyA".into();
        assert!(validate(std::slice::from_ref(&other)).is_err());
        other.shortcut = "Shift+KeyA".into();
        assert!(validate(std::slice::from_ref(&other)).is_err());
        other.shortcut = "Super+Alt+KeyA".into();
        assert!(validate(&[other]).is_ok());
    }
}

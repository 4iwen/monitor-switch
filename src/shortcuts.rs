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
            .map_err(|error| format!("快捷键 {} 无效：{error}", binding.shortcut))?;
        if !hotkey
            .mods
            .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
        {
            return Err("快捷键必须包含 Control、Alt/Option 或 Command/Win 修饰键".into());
        }
        if !targets.insert((binding.display.clone(), binding.value)) {
            return Err("同一个输入源只能设置一个快捷键".into());
        }
        if parsed
            .insert(hotkey.id(), (hotkey, binding.clone()))
            .is_some()
        {
            return Err(format!("快捷键 {} 重复，请选择其他组合", binding.shortcut));
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
                    "无法注册 {}，可能已被占用：{error}",
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

#[cfg(test)]
mod tests {
    use super::*;
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

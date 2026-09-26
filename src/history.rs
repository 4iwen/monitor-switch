use std::collections::HashMap;

#[derive(Default)]
pub struct InputHistory {
    entries: HashMap<String, (u16, Option<u16>)>,
}

impl InputHistory {
    // Call only after a successful read or an accepted switch command.
    pub fn observe(&mut self, display: &str, value: u16) {
        let entry = self
            .entries
            .entry(display.to_owned())
            .or_insert((value, None));
        if entry.0 != value {
            *entry = (value, Some(entry.0));
        }
    }

    pub fn previous(&self) -> HashMap<String, u16> {
        self.entries
            .iter()
            .filter_map(|(display, (_, previous))| previous.map(|value| (display.clone(), value)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::InputHistory;

    #[test]
    fn repeated_reads_do_not_erase_history_and_displays_are_independent() {
        let mut history = InputHistory::default();
        history.observe("a", 17);
        history.observe("a", 18);
        history.observe("a", 18);
        history.observe("b", 15);
        assert_eq!(history.previous().get("a"), Some(&17));
        assert!(!history.previous().contains_key("b"));
        history.observe("a", 17);
        assert_eq!(history.previous().get("a"), Some(&18));
    }
}

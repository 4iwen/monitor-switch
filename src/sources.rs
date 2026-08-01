use crate::error::{AppError, Result};

#[derive(Debug, Clone, Copy)]
pub struct InputCandidate {
    pub value: u16,
    pub name: &'static str,
    pub note: &'static str,
}

pub const INPUT_SOURCE_VCP_CODE: u8 = 0x60;

pub const COMMON_INPUT_CANDIDATES: &[InputCandidate] = &[
    InputCandidate {
        value: 0x01,
        name: "VGA 1",
        note: "Analog video, older monitors",
    },
    InputCandidate {
        value: 0x02,
        name: "VGA 2",
        note: "Analog video, older monitors",
    },
    InputCandidate {
        value: 0x03,
        name: "DVI 1",
        note: "Digital video, older monitors",
    },
    InputCandidate {
        value: 0x04,
        name: "DVI 2",
        note: "Digital video, older monitors",
    },
    InputCandidate {
        value: 0x0f,
        name: "DisplayPort 1",
        note: "Common DP input value",
    },
    InputCandidate {
        value: 0x10,
        name: "DisplayPort 2",
        note: "Common DP input value",
    },
    InputCandidate {
        value: 0x11,
        name: "HDMI 1",
        note: "Common HDMI input value",
    },
    InputCandidate {
        value: 0x12,
        name: "HDMI 2",
        note: "Common HDMI input value",
    },
    InputCandidate {
        value: 0x13,
        name: "HDMI 3",
        note: "Seen on some multi-HDMI monitors",
    },
    InputCandidate {
        value: 0x14,
        name: "HDMI 4",
        note: "Seen on some multi-HDMI monitors",
    },
    InputCandidate {
        value: 0x1b,
        name: "USB-C",
        note: "Common USB-C / DP Alt Mode value",
    },
];

pub fn parse_input_value(raw: &str) -> Result<u16> {
    let trimmed = raw.trim();

    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return u16::from_str_radix(hex, 16)
            .map_err(|_| AppError::InvalidInputValue(raw.to_string()));
    }

    trimmed
        .parse::<u16>()
        .map_err(|_| AppError::InvalidInputValue(raw.to_string()))
}

pub fn format_input_value(value: u16) -> String {
    format!("0x{value:02x}")
}

#[cfg(test)]
mod tests {
    use super::{format_input_value, parse_input_value};

    #[test]
    fn parses_decimal_input_values() {
        assert_eq!(parse_input_value("17").unwrap(), 17);
    }

    #[test]
    fn parses_hex_input_values() {
        assert_eq!(parse_input_value("0x11").unwrap(), 17);
        assert_eq!(parse_input_value("0X60").unwrap(), 96);
    }

    #[test]
    fn formats_hex_input_values() {
        assert_eq!(format_input_value(17), "0x11");
        assert_eq!(format_input_value(96), "0x60");
    }
}

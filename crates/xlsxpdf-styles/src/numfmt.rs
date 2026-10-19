/// Format a numeric cell value according to an Excel number format code.
/// This is a v1 implementation covering the most common built-in formats.
pub fn format_number(value: f64, num_fmt_id: u32, custom_code: Option<&str>) -> String {
    if let Some(code) = custom_code {
        return format_with_code(value, code);
    }

    match num_fmt_id {
        0 => format_general(value),
        1 => format!("{}", value as i64),
        2 => format!("{:.2}", value),
        3 => format_thousands(value, 0),
        4 => format_thousands(value, 2),
        9 => format!("{}%", (value * 100.0) as i64),
        10 => format!("{:.2}%", value * 100.0),
        11 => format_scientific(value),
        12 => {
            // Fraction — simplified
            format_general(value)
        }
        14 => format_date(value, "m/d/yyyy"),
        15 => format_date(value, "d-mmm-yy"),
        16 => format_date(value, "d-mmm"),
        17 => format_date(value, "mmm-yy"),
        18 => format_time(value, "h:mm AM/PM"),
        19 => format_time(value, "h:mm:ss AM/PM"),
        20 => format_time(value, "h:mm"),
        21 => format_time(value, "h:mm:ss"),
        22 => format_datetime(value),
        37 => format_thousands_parens(value, 0),
        38 => format_thousands_parens(value, 0),
        39 => format_thousands_parens(value, 2),
        40 => format_thousands_parens(value, 2),
        49 => format_general(value), // Text format — pass through
        _ => format_general(value),
    }
}

fn format_general(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    if value == (value as i64) as f64 && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let s = format!("{}", value);
    if s.len() > 11 {
        format!("{:.6}", value)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    } else {
        s
    }
}

fn format_thousands(value: f64, decimals: usize) -> String {
    let negative = value < 0.0;
    let abs = value.abs();
    let formatted = if decimals > 0 {
        format!("{:.prec$}", abs, prec = decimals)
    } else {
        format!("{}", abs.round() as i64)
    };

    let parts: Vec<&str> = formatted.splitn(2, '.').collect();
    let int_part = parts[0];
    let dec_part = parts.get(1);

    let mut result = String::new();
    let digits: Vec<char> = int_part.chars().collect();
    for (i, ch) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(*ch);
    }

    if let Some(dec) = dec_part {
        result.push('.');
        result.push_str(dec);
    }

    if negative {
        format!("-{result}")
    } else {
        result
    }
}

fn format_thousands_parens(value: f64, decimals: usize) -> String {
    if value < 0.0 {
        format!("({})", format_thousands(-value, decimals))
    } else {
        format_thousands(value, decimals)
    }
}

fn format_scientific(value: f64) -> String {
    format!("{:.2E}", value)
}

fn format_date(serial: f64, _pattern: &str) -> String {
    let (year, month, day) = serial_to_date(serial as i64);
    format!("{}/{}/{}", month, day, year)
}

fn format_time(serial: f64, _pattern: &str) -> String {
    let frac = serial.fract().abs();
    let total_seconds = (frac * 86400.0).round() as u32;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours}:{minutes:02}:{seconds:02}")
}

fn format_datetime(serial: f64) -> String {
    let date = format_date(serial, "");
    let time = format_time(serial, "");
    format!("{date} {time}")
}

fn serial_to_date(serial: i64) -> (i32, u32, u32) {
    // Excel serial date: 1 = 1900-01-01
    // Account for the Lotus 1-2-3 leap year bug (1900 is not a leap year
    // but Excel treats it as one)
    let serial = if serial > 59 { serial - 1 } else { serial };
    let days_since_epoch = serial - 1;

    let mut year = 1900;
    let mut remaining = days_since_epoch;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let leap = is_leap_year(year);
    let month_days: [i64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0u32;
    for (i, &days) in month_days.iter().enumerate() {
        if remaining < days {
            month = i as u32 + 1;
            break;
        }
        remaining -= days;
    }
    if month == 0 {
        month = 12;
    }

    (year, month, remaining as u32 + 1)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn format_with_code(value: f64, code: &str) -> String {
    // Handle multi-section codes: positive;negative;zero;text
    let sections: Vec<&str> = code.split(';').collect();
    let section = if sections.len() >= 3 && value == 0.0 {
        sections[2]
    } else if sections.len() >= 2 && value < 0.0 {
        sections[1]
    } else {
        sections[0]
    };
    let format_value = if sections.len() >= 2 && value < 0.0 {
        value.abs()
    } else {
        value
    };

    format_section(format_value, section)
}

fn format_section(value: f64, section: &str) -> String {
    // Strip literal strings in quotes and _ (space padding) and * (fill chars)
    let cleaned = strip_format_literals(section);
    let code_lower = cleaned.to_lowercase();

    if code_lower.contains('%') {
        let pct = value * 100.0;
        let decimals = cleaned.matches('0').count().saturating_sub(1);
        return format!("{:.prec$}%", pct, prec = decimals);
    }

    if code_lower.contains("e+") || code_lower.contains("e-") {
        return format_scientific(value);
    }

    if code_lower.contains("yy") || code_lower.contains("mm") && code_lower.contains("dd") {
        return format_date(value, &cleaned);
    }

    if code_lower.contains("h:") || code_lower.contains(":mm") {
        return format_time(value, &cleaned);
    }

    if cleaned.contains('#') || cleaned.contains('0') {
        let has_comma = cleaned.contains(',');
        let decimal_places = cleaned
            .rsplit_once('.')
            .map(|(_, after)| after.chars().take_while(|c| *c == '0' || *c == '#').count())
            .unwrap_or(0);

        let mut result = if has_comma {
            format_thousands(value, decimal_places)
        } else if decimal_places > 0 {
            format!("{:.prec$}", value, prec = decimal_places)
        } else {
            format_general(value)
        };

        // Apply prefix/suffix literals from original section
        let (prefix, suffix) = extract_literals(section);
        if !prefix.is_empty() || !suffix.is_empty() {
            result = format!("{prefix}{result}{suffix}");
        }

        return result;
    }

    format_general(value)
}

fn strip_format_literals(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut chars = code.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                while let Some(c) = chars.next() {
                    if c == '"' { break; }
                }
            }
            '_' | '*' => { chars.next(); }
            '\\' => { chars.next(); }
            '$' => {}
            _ => out.push(ch),
        }
    }
    out
}

fn extract_literals(code: &str) -> (String, String) {
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut in_numeric = false;
    let mut chars = code.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '#' | '0' | '.' | ',' | '%' | 'E' | 'e' => { in_numeric = true; }
            '"' => {
                let mut lit = String::new();
                while let Some(c) = chars.next() {
                    if c == '"' { break; }
                    lit.push(c);
                }
                if in_numeric { suffix.push_str(&lit); } else { prefix.push_str(&lit); }
            }
            '\\' => {
                if let Some(c) = chars.next() {
                    if in_numeric { suffix.push(c); } else { prefix.push(c); }
                }
            }
            '_' | '*' => { chars.next(); }
            '$' | '(' | ')' | '-' | '+' | ' ' => {
                if in_numeric { suffix.push(ch); } else { prefix.push(ch); }
            }
            _ => {}
        }
    }
    (prefix, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_general() {
        assert_eq!(format_number(42.0, 0, None), "42");
        assert_eq!(format_number(3.14, 0, None), "3.14");
    }

    #[test]
    fn test_integer() {
        assert_eq!(format_number(42.7, 1, None), "42");
    }

    #[test]
    fn test_decimal() {
        assert_eq!(format_number(42.7, 2, None), "42.70");
    }

    #[test]
    fn test_thousands() {
        assert_eq!(format_number(1234567.0, 3, None), "1,234,567");
        assert_eq!(format_number(1234.56, 4, None), "1,234.56");
    }

    #[test]
    fn test_percent() {
        assert_eq!(format_number(0.153, 9, None), "15%");
        assert_eq!(format_number(0.153, 10, None), "15.30%");
    }

    #[test]
    fn test_date() {
        // 44197 = 2021-01-01 in Excel serial
        assert_eq!(format_number(44197.0, 14, None), "1/1/2021");
    }

    #[test]
    fn test_custom_code() {
        assert_eq!(
            format_number(1234.5, 0, Some("#,##0.00")),
            "1,234.50"
        );
        assert_eq!(
            format_number(0.153, 0, Some("0.00%")),
            "15.30%"
        );
    }

    #[test]
    fn test_multi_section_negative() {
        let code = "#,##0.00;(#,##0.00)";
        assert_eq!(format_number(1234.5, 0, Some(code)), "1,234.50");
        assert_eq!(format_number(-1234.5, 0, Some(code)), "(1,234.50)");
    }

    #[test]
    fn test_multi_section_zero() {
        let code = "#,##0.00;-#,##0.00;0.00";
        assert_eq!(format_number(0.0, 0, Some(code)), "0.00");
    }

    #[test]
    fn test_currency_literal() {
        let code = "\"$\"#,##0.00";
        assert_eq!(format_number(1234.5, 0, Some(code)), "$1,234.50");
    }
}

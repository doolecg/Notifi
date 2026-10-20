//! Calculator expressions and unit conversion for the Command Bar.

use std::f64::consts::{E, PI};

/// Evaluates `input` when it reads as a calculation (it must hold an operator or a function).
pub fn evaluate(input: &str) -> Option<String> {
    let text = input.trim().trim_end_matches('=').trim();
    if text.is_empty() || looks_like_date_or_time(text) {
        return None;
    }
    let tokens = tokenize(text)?;
    let calculation = tokens.iter().any(|t| matches!(t, Token::Op(_) | Token::Func(_) | Token::Percent))
        || tokens.iter().any(|t| matches!(t, Token::Const(_)));
    if !calculation {
        return None;
    }
    let mut parser = Parser { tokens, pos: 0 };
    let value = parser.expression()?;
    if parser.pos != parser.tokens.len() || !value.is_finite() {
        return None;
    }
    Some(format_number(value))
}

fn looks_like_date_or_time(text: &str) -> bool {
    let b = text.as_bytes();
    let mut groups = 0;
    let mut i = 0;
    let mut sep = None;
    while i < b.len() {
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
        groups += 1;
        if i == b.len() {
            break;
        }
        let s = b[i];
        if !matches!(s, b'/' | b'-' | b'.' | b':') || sep.is_some_and(|p| p != s) {
            return false;
        }
        sep = Some(s);
        i += 1;
    }
    match sep {
        Some(b':') => groups >= 2,
        Some(b'/') | Some(b'-') => groups == 3,
        Some(b'.') => groups == 3 && text.split('.').all(|g| g.len() <= 4),
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Op(char),
    Percent,
    LParen,
    RParen,
    Func(&'static str),
    Const(f64),
}

const FUNCS: &[&str] = &[
    "sqrt", "cbrt", "sin", "cos", "tan", "asin", "acos", "atan", "ln", "log", "exp", "abs", "floor", "ceil", "round",
];

fn tokenize(text: &str) -> Option<Vec<Token>> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\u{a0}' => i += 1,
            '0'..='9' | '.' | ',' => {
                let (value, next) = read_number(&chars, i)?;
                out.push(Token::Num(value));
                i = next;
            }
            '+' | '-' | '*' | '/' | '^' => {
                out.push(Token::Op(c));
                i += 1;
            }
            '×' | 'x' | 'X' if matches!(out.last(), Some(Token::Num(_) | Token::RParen | Token::Percent)) => {
                out.push(Token::Op('*'));
                i += 1;
            }
            '÷' => {
                out.push(Token::Op('/'));
                i += 1;
            }
            '−' => {
                out.push(Token::Op('-'));
                i += 1;
            }
            '%' => {
                out.push(Token::Percent);
                i += 1;
            }
            '(' => {
                out.push(Token::LParen);
                i += 1;
            }
            ')' => {
                out.push(Token::RParen);
                i += 1;
            }
            'π' => {
                out.push(Token::Const(PI));
                i += 1;
            }
            c if c.is_alphabetic() => {
                let start = i;
                while i < chars.len() && chars[i].is_alphabetic() {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
                if let Some(f) = FUNCS.iter().find(|f| **f == word) {
                    out.push(Token::Func(f));
                } else if word == "pi" {
                    out.push(Token::Const(PI));
                } else if word == "e" {
                    out.push(Token::Const(E));
                } else if word == "mod" {
                    out.push(Token::Op('m'));
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

fn grouped(int: &str, groups: &[&str]) -> bool {
    !int.is_empty()
        && int.len() <= 3
        && !int.starts_with('0')
        && int.bytes().all(|b| b.is_ascii_digit())
        && groups.iter().all(|g| g.len() == 3 && g.bytes().all(|b| b.is_ascii_digit()))
}

/// Reads one number starting at `start`, handling decimal separators of either style and
/// thousands grouping with space, apostrophe, comma or dot. Misplaced separators fail.
fn read_number(chars: &[char], start: usize) -> Option<(f64, usize)> {
    let mut i = start;
    let mut raw = String::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() || c == '.' || c == ',' {
            raw.push(c);
            i += 1;
        } else if matches!(c, ' ' | '\'' | '\u{a0}')
            && chars.get(i + 1..i + 4).is_some_and(|g| g.iter().all(char::is_ascii_digit))
            && !chars.get(i + 4).is_some_and(char::is_ascii_digit)
            && !raw.contains(['.', ','])
            && raw.split('_').next().is_some_and(|first| grouped(first, &[]))
        {
            raw.push('_');
            i += 1;
        } else {
            break;
        }
    }
    Some((parse_grouped(&raw)?, i))
}

fn parse_grouped(raw: &str) -> Option<f64> {
    if raw.is_empty() {
        return None;
    }
    if raw.contains('_') {
        let parts: Vec<&str> = raw.split('_').collect();
        let (last, middle) = parts.split_last()?;
        let (tail, frac) = match last.split_once(['.', ',']) {
            Some((t, f)) if !f.contains(['.', ',']) => (t, f),
            Some(_) => return None,
            None => (*last, ""),
        };
        if tail.len() != 3 || !grouped(middle[0], &middle[1..]) {
            return None;
        }
        let int = format!("{}{}", middle.concat(), tail);
        return if frac.is_empty() { int.parse().ok() } else { format!("{int}.{frac}").parse().ok() };
    }
    let dots = raw.matches('.').count();
    let commas = raw.matches(',').count();
    let (decimal, group): (Option<char>, Option<char>) = if dots > 0 && commas > 0 {
        let last = raw.rfind(['.', ','])?;
        let ch = raw[last..].chars().next()?;
        let other = if ch == '.' { ',' } else { '.' };
        if raw.matches(ch).count() != 1 || raw.rfind(other)? > last {
            return None;
        }
        (Some(ch), Some(other))
    } else if dots + commas == 0 {
        (None, None)
    } else {
        let sep = if dots > 0 { '.' } else { ',' };
        if dots + commas > 1 {
            (None, Some(sep))
        } else {
            let (int, frac) = raw.split_once(sep)?;
            if sep == ',' && frac.len() == 3 && grouped(int, &[]) { (None, Some(sep)) } else { (Some(sep), None) }
        }
    };
    let (int_part, frac_part) = match decimal {
        Some(sep) => raw.split_once(sep)?,
        None => (raw, ""),
    };
    let int_digits = match group {
        Some(g) if int_part.contains(g) => {
            let parts: Vec<&str> = int_part.split(g).collect();
            if !grouped(parts[0], &parts[1..]) {
                return None;
            }
            parts.concat()
        }
        _ => int_part.to_string(),
    };
    if int_digits.contains(['.', ',']) || frac_part.contains(['.', ',']) {
        return None;
    }
    let int_digits = if int_digits.is_empty() { "0".to_string() } else { int_digits };
    if frac_part.is_empty() { int_digits.parse().ok() } else { format!("{int_digits}.{frac_part}").parse().ok() }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn eat_op(&mut self, ops: &[char]) -> Option<char> {
        match self.peek() {
            Some(Token::Op(c)) if ops.contains(c) => {
                let c = *c;
                self.pos += 1;
                Some(c)
            }
            _ => None,
        }
    }

    fn expression(&mut self) -> Option<f64> {
        let mut left = self.term()?;
        while let Some(op) = self.eat_op(&['+', '-']) {
            let before = self.pos;
            let right = self.term()?;
            let percent = matches!(self.tokens.get(self.pos - 1), Some(Token::Percent)) && self.pos > before;
            let right = if percent { left * right } else { right };
            left = if op == '+' { left + right } else { left - right };
        }
        Some(left)
    }

    fn term(&mut self) -> Option<f64> {
        let mut left = self.unary()?;
        loop {
            if let Some(op) = self.eat_op(&['*', '/', 'm']) {
                let right = self.unary()?;
                left = match op {
                    '*' => left * right,
                    '/' => left / right,
                    _ => left % right,
                };
            } else if matches!(self.peek(), Some(Token::LParen | Token::Func(_) | Token::Const(_))) {
                left *= self.unary()?;
            } else {
                return Some(left);
            }
        }
    }

    fn unary(&mut self) -> Option<f64> {
        if self.eat_op(&['-']).is_some() {
            return self.unary().map(|v| -v);
        }
        if self.eat_op(&['+']).is_some() {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.postfix()?;
        if self.eat_op(&['^']).is_some() {
            let exp = self.unary()?;
            return Some(base.powf(exp));
        }
        Some(base)
    }

    fn postfix(&mut self) -> Option<f64> {
        let mut value = self.atom()?;
        while matches!(self.peek(), Some(Token::Percent)) {
            self.pos += 1;
            value /= 100.0;
        }
        Some(value)
    }

    fn atom(&mut self) -> Option<f64> {
        match self.tokens.get(self.pos)?.clone() {
            Token::Num(n) => {
                self.pos += 1;
                Some(n)
            }
            Token::Const(c) => {
                self.pos += 1;
                Some(c)
            }
            Token::LParen => {
                self.pos += 1;
                let v = self.expression()?;
                match self.tokens.get(self.pos) {
                    Some(Token::RParen) => self.pos += 1,
                    None => {}
                    _ => return None,
                }
                Some(v)
            }
            Token::Func(name) => {
                self.pos += 1;
                let arg = if matches!(self.peek(), Some(Token::LParen)) { self.atom()? } else { self.power()? };
                apply(name, arg)
            }
            _ => None,
        }
    }
}

fn apply(name: &str, x: f64) -> Option<f64> {
    Some(match name {
        "sqrt" if x < 0.0 => return None,
        "sqrt" => x.sqrt(),
        "cbrt" => x.cbrt(),
        "sin" => x.sin(),
        "cos" => x.cos(),
        "tan" => x.tan(),
        "asin" => x.asin(),
        "acos" => x.acos(),
        "atan" => x.atan(),
        "ln" => x.ln(),
        "log" => x.log10(),
        "exp" => x.exp(),
        "abs" => x.abs(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "round" => x.round(),
        _ => return None,
    })
}

/// Keeps meaningful digits and hides rounding noise (0.1 + 0.2 shows 0.3).
pub fn format_number(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let abs = value.abs();
    if !(1e-9..1e15).contains(&abs) {
        let s = format!("{value:.11e}");
        let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
        let mant = mant.trim_end_matches('0').trim_end_matches('.');
        return format!("{mant}e{exp}");
    }
    let digits = 12_i32;
    let magnitude = abs.log10().floor() as i32;
    let decimals = (digits - 1 - magnitude).clamp(0, 15) as usize;
    let s = format!("{value:.decimals$}");
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

// ---- Units ----

#[derive(Clone, Copy, PartialEq, Debug)]
enum Category {
    Length,
    Mass,
    Temperature,
    Volume,
    Data,
    Time,
}

struct Unit {
    names: &'static [&'static str],
    category: Category,
    factor: f64,
    label: &'static str,
}

const fn u(names: &'static [&'static str], category: Category, factor: f64, label: &'static str) -> Unit {
    Unit { names, category, factor, label }
}

use Category::*;

const UNITS: &[Unit] = &[
    u(&["mm", "millimeter", "millimeters", "millimetre", "millimetres"], Length, 0.001, "mm"),
    u(&["cm", "centimeter", "centimeters", "centimetre", "centimetres"], Length, 0.01, "cm"),
    u(&["m", "meter", "meters", "metre", "metres"], Length, 1.0, "m"),
    u(&["km", "kilometer", "kilometers", "kilometre", "kilometres"], Length, 1000.0, "km"),
    u(&["in", "inch", "inches"], Length, 0.0254, "in"),
    u(&["ft", "foot", "feet"], Length, 0.3048, "ft"),
    u(&["yd", "yard", "yards"], Length, 0.9144, "yd"),
    u(&["mi", "mile", "miles"], Length, 1609.344, "mi"),
    u(&["mg", "milligram", "milligrams"], Mass, 0.000001, "mg"),
    u(&["g", "gram", "grams"], Mass, 0.001, "g"),
    u(&["kg", "kilogram", "kilograms", "kilo", "kilos"], Mass, 1.0, "kg"),
    u(&["t", "tonne", "tonnes"], Mass, 1000.0, "t"),
    u(&["oz", "ounce", "ounces"], Mass, 0.028349523125, "oz"),
    u(&["lb", "lbs", "pound", "pounds"], Mass, 0.45359237, "lb"),
    u(&["st", "stone", "stones"], Mass, 6.35029318, "st"),
    u(&["c", "celsius", "°c"], Temperature, 1.0, "°C"),
    u(&["f", "fahrenheit", "°f"], Temperature, 1.0, "°F"),
    u(&["k", "kelvin"], Temperature, 1.0, "K"),
    u(&["ml", "milliliter", "milliliters", "millilitre", "millilitres"], Volume, 0.001, "ml"),
    u(&["l", "liter", "liters", "litre", "litres"], Volume, 1.0, "l"),
    u(&["tsp", "teaspoon", "teaspoons"], Volume, 0.00492892159375, "tsp"),
    u(&["tbsp", "tablespoon", "tablespoons"], Volume, 0.01478676478125, "tbsp"),
    u(&["floz", "fl oz"], Volume, 0.0295735295625, "fl oz"),
    u(&["cup", "cups"], Volume, 0.2365882365, "cup"),
    u(&["pt", "pint", "pints"], Volume, 0.473176473, "pt"),
    u(&["qt", "quart", "quarts"], Volume, 0.946352946, "qt"),
    u(&["gal", "gallon", "gallons"], Volume, 3.785411784, "gal"),
    u(&["b", "byte", "bytes"], Data, 1.0, "B"),
    u(&["kb", "kilobyte", "kilobytes"], Data, 1e3, "KB"),
    u(&["mb", "megabyte", "megabytes"], Data, 1e6, "MB"),
    u(&["gb", "gigabyte", "gigabytes"], Data, 1e9, "GB"),
    u(&["tb", "terabyte", "terabytes"], Data, 1e12, "TB"),
    u(&["kib", "kibibyte", "kibibytes"], Data, 1024.0, "KiB"),
    u(&["mib", "mebibyte", "mebibytes"], Data, 1_048_576.0, "MiB"),
    u(&["gib", "gibibyte", "gibibytes"], Data, 1_073_741_824.0, "GiB"),
    u(&["tib", "tebibyte", "tebibytes"], Data, 1_099_511_627_776.0, "TiB"),
    u(&["bit", "bits"], Data, 0.125, "bit"),
    u(&["ms", "millisecond", "milliseconds"], Time, 0.001, "ms"),
    u(&["s", "sec", "secs", "second", "seconds"], Time, 1.0, "s"),
    u(&["min", "mins", "minute", "minutes"], Time, 60.0, "min"),
    u(&["h", "hr", "hrs", "hour", "hours"], Time, 3600.0, "h"),
    u(&["d", "day", "days"], Time, 86400.0, "d"),
    u(&["wk", "week", "weeks"], Time, 604_800.0, "wk"),
    u(&["yr", "year", "years"], Time, 31_557_600.0, "yr"),
];

fn find_unit(name: &str) -> Option<&'static Unit> {
    let name = name.trim().to_lowercase();
    UNITS.iter().find(|u| u.names.contains(&name.as_str()))
}

fn to_celsius(v: f64, label: &str) -> f64 {
    match label {
        "°F" => (v - 32.0) * 5.0 / 9.0,
        "K" => v - 273.15,
        _ => v,
    }
}

fn from_celsius(v: f64, label: &str) -> f64 {
    match label {
        "°F" => v * 9.0 / 5.0 + 32.0,
        "K" => v + 273.15,
        _ => v,
    }
}

/// "10 km to mi" gives "10 km = 6.2137119 mi" and the bare result for copying.
pub fn convert(input: &str) -> Option<(String, String)> {
    let text = input.trim().to_lowercase();
    let (left, to) = split_on_keyword(&text)?;
    let split = left.find(|c: char| c.is_alphabetic() || c == '°')?;
    let (num_text, from_text) = left.split_at(split);
    let value = match num_text.trim() {
        "" => 1.0,
        n => {
            let tokens = tokenize(n)?;
            let mut p = Parser { tokens, pos: 0 };
            let v = p.expression()?;
            if p.pos != p.tokens.len() {
                return None;
            }
            v
        }
    };
    let from = find_unit(from_text)?;
    let target = find_unit(to)?;
    if from.category != target.category {
        return None;
    }
    let result = if from.category == Temperature {
        from_celsius(to_celsius(value, from.label), target.label)
    } else {
        value * from.factor / target.factor
    };
    let shown = format_number(result);
    Some((format!("{} {} = {} {}", format_number(value), from.label, shown, target.label), shown))
}

fn split_on_keyword(text: &str) -> Option<(&str, &str)> {
    for key in [" to ", " in ", " as ", "=>", " into "] {
        if let Some((a, b)) = text.split_once(key) {
            if !a.trim().is_empty() && !b.trim().is_empty() {
                return Some((a, b));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc(s: &str) -> Option<String> {
        evaluate(s)
    }

    #[test]
    fn basic_and_chained() {
        assert_eq!(calc("2+3*4").as_deref(), Some("14"));
        assert_eq!(calc("100 - 20 - 30").as_deref(), Some("50"));
        assert_eq!(calc("8 / 2 / 2").as_deref(), Some("2"));
        assert_eq!(calc("2^3^2").as_deref(), Some("512"));
        assert_eq!(calc("(1+2)(3+4)").as_deref(), Some("21"));
        assert_eq!(calc("-3 + 5").as_deref(), Some("2"));
    }

    #[test]
    fn rounding_noise_is_hidden() {
        assert_eq!(calc("0.1 + 0.2").as_deref(), Some("0.3"));
        assert_eq!(calc("1/3").as_deref(), Some("0.333333333333"));
        assert_eq!(calc("10/4").as_deref(), Some("2.5"));
    }

    #[test]
    fn percentages() {
        assert_eq!(calc("50%").as_deref(), Some("0.5"));
        assert_eq!(calc("200 + 10%").as_deref(), Some("220"));
        assert_eq!(calc("200 - 25%").as_deref(), Some("150"));
        assert_eq!(calc("200 * 10%").as_deref(), Some("20"));
    }

    #[test]
    fn functions_and_constants() {
        assert_eq!(calc("sqrt(16)").as_deref(), Some("4"));
        assert_eq!(calc("sqrt 9 + 1").as_deref(), Some("4"));
        assert_eq!(calc("2pi").map(|s| s.starts_with("6.2831853071")), Some(true));
        assert_eq!(calc("sin(0)").as_deref(), Some("0"));
        assert_eq!(calc("7 mod 4").as_deref(), Some("3"));
        assert_eq!(calc("sqrt(-1)"), None);
    }

    #[test]
    fn decimal_separators_and_grouping() {
        assert_eq!(calc("1,5 + 1").as_deref(), Some("2.5"));
        assert_eq!(calc("1.5 + 1").as_deref(), Some("2.5"));
        assert_eq!(calc("1 000 + 1").as_deref(), Some("1001"));
        assert_eq!(calc("1'000'000 / 4").as_deref(), Some("250000"));
        assert_eq!(calc("1,234.5 + 0").as_deref(), Some("1234.5"));
        assert_eq!(calc("1.234,5 + 0").as_deref(), Some("1234.5"));
        assert_eq!(calc("1,234 + 1").as_deref(), Some("1235"));
    }

    #[test]
    fn misplaced_separators_are_rejected() {
        assert_eq!(calc("1,23,456 + 1"), None);
        assert_eq!(calc("12,34,567.8 + 1"), None);
        assert_eq!(calc("1.2.3 + 1"), None);
        assert_eq!(calc("0 123 + 1"), None);
        assert_eq!(calc("1 2345 + 1"), None);
    }

    #[test]
    fn leading_zero_is_not_thousands() {
        assert_eq!(calc("0,123 + 1").as_deref(), Some("1.123"));
        assert_eq!(calc("0.123 + 1").as_deref(), Some("1.123"));
    }

    #[test]
    fn dates_and_times_are_not_sums() {
        assert_eq!(calc("12-05-2024"), None);
        assert_eq!(calc("2024/05/12"), None);
        assert_eq!(calc("10:30"), None);
        assert_eq!(calc("3.5.2024"), None);
        assert_eq!(calc("7"), None);
        assert_eq!(calc("firefox"), None);
    }

    #[test]
    fn divide_by_zero_fails() {
        assert_eq!(calc("1/0"), None);
    }

    #[test]
    fn units() {
        assert_eq!(convert("10 km to mi").unwrap().1, "6.21371192237");
        assert_eq!(convert("1 kg in lb").unwrap().1, "2.20462262185");
        assert_eq!(convert("100 c to f").unwrap().1, "212");
        assert_eq!(convert("32 f to c").unwrap().1, "0");
        assert_eq!(convert("0 c to k").unwrap().1, "273.15");
        assert_eq!(convert("2 hours to minutes").unwrap().1, "120");
        assert_eq!(convert("1.5 gb to mb").unwrap().1, "1500");
        assert_eq!(convert("1 gib to mib").unwrap().1, "1024");
        assert_eq!(convert("1 gal to l").unwrap().1, "3.785411784");
        assert!(convert("1 km to kg").is_none());
        assert!(convert("hello to world").is_none());
        assert_eq!(convert("km to m").unwrap().1, "1000");
    }
}

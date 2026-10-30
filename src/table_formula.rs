//! Small, safe arithmetic evaluator used by the explicit table formula bar.
//!
//! Supported grammar: numbers, cell references like A1, + - * /, parentheses.
//! No functions, names, ranges, macros or network access are processed. A
//! normal cell string beginning with '=' is still literal text; evaluation is
//! only performed by the explicit Formula action.
use std::fmt;

const MAX_EXPRESSION_BYTES: usize = 512;
const MAX_TOKENS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormulaError {
    Empty,
    TooLong,
    BadReference,
    BadNumber,
    Unexpected,
    DivideByZero,
    NotFinite,
    NonNumericCell(String),
}

impl fmt::Display for FormulaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "formula is empty"),
            Self::TooLong => write!(f, "formula is too long"),
            Self::BadReference => write!(f, "invalid cell reference"),
            Self::BadNumber => write!(f, "invalid number"),
            Self::Unexpected => write!(f, "unexpected formula token"),
            Self::DivideByZero => write!(f, "division by zero"),
            Self::NotFinite => write!(f, "result is not finite"),
            Self::NonNumericCell(reference) => write!(f, "cell {reference} is not numeric"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
    Left,
    Right,
}

pub fn parse_cell_reference(reference: &str) -> Option<(usize, usize)> {
    let reference = reference.trim().trim_start_matches('$');
    let split = reference.find(|c: char| c.is_ascii_digit())?;
    let (column, row) = reference.split_at(split);
    if column.is_empty() || row.is_empty() || !row.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut column_index = 0usize;
    for character in column.chars().filter(|c| *c != '$') {
        if !character.is_ascii_alphabetic() {
            return None;
        }
        column_index = column_index
            .checked_mul(26)?
            .checked_add(character.to_ascii_uppercase() as usize - 'A' as usize + 1)?;
    }
    let row_number = row.parse::<usize>().ok()?;
    if row_number == 0 || column_index == 0 {
        return None;
    }
    Some((row_number - 1, column_index - 1))
}

pub fn format_number(value: f64, decimals: Option<u8>) -> String {
    let Some(decimals) = decimals else {
        let text = value.to_string();
        return if text == "-0" { "0".into() } else { text };
    };
    let places = decimals.min(15) as usize;
    let mut text = format!("{value:.places$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        "0".into()
    } else {
        text
    }
}

pub fn shift_references(
    expression: &str,
    min_row: usize,
    row_delta: isize,
    min_column: usize,
    column_delta: isize,
) -> String {
    let bytes = expression.as_bytes();
    let mut output = String::with_capacity(expression.len());
    let mut index = 0;
    while index < bytes.len() {
        let start = index;
        let mut cursor = index;
        let absolute_column = bytes.get(cursor) == Some(&b'$');
        if absolute_column {
            cursor += 1;
        }
        let letters_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_alphabetic) {
            cursor += 1;
        }
        if cursor == letters_start {
            output.push(bytes[index] as char);
            index += 1;
            continue;
        }
        let letters = &expression[letters_start..cursor];
        let absolute_row = bytes.get(cursor) == Some(&b'$');
        if absolute_row {
            cursor += 1;
        }
        let digits_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == digits_start {
            output.push(bytes[index] as char);
            index += 1;
            continue;
        }
        let row = expression[digits_start..cursor].parse::<usize>().ok();
        let column = column_index(letters);
        if let (Some(mut row), Some(mut column)) = (row, column) {
            if row >= min_row {
                row = (row as isize + row_delta).max(1) as usize;
            }
            if column >= min_column {
                column = (column as isize + column_delta).max(1) as usize;
            }
            output.push_str(&expression[start..letters_start]);
            output.push_str(&column_letters(column));
            if absolute_row {
                output.push('$');
            }
            output.push_str(&row.to_string());
            index = cursor;
            continue;
        }
        output.push(bytes[index] as char);
        index += 1;
    }
    output
}

fn column_index(letters: &str) -> Option<usize> {
    let mut value = 0usize;
    for letter in letters.bytes() {
        if !letter.is_ascii_alphabetic() {
            return None;
        }
        value = value.checked_mul(26)? + (letter.to_ascii_uppercase() - b'A' + 1) as usize;
    }
    (value > 0 && value <= 16_384).then_some(value)
}

fn column_letters(mut column: usize) -> String {
    column = column.min(16_384);
    let mut output = String::new();
    while column > 0 {
        let remainder = (column - 1) % 26;
        output.insert(0, (b'A' + remainder as u8) as char);
        column = (column - 1) / 26;
    }
    output
}

pub fn is_simple_formula(expression: &str) -> bool {
    // Syntax validation must not depend on the *values* of referenced cells.
    // A non-zero placeholder keeps ordinary formulas like =10/A1 valid;
    // division by zero is a runtime condition, not a syntax error.
    match evaluate(expression, |_| Some("1".into())) {
        Ok(_) | Err(FormulaError::DivideByZero) => true,
        Err(_) => false,
    }
}

pub fn evaluate(
    expression: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<f64, FormulaError> {
    let expression = expression.trim().trim_start_matches('=').trim();
    if expression.is_empty() {
        return Err(FormulaError::Empty);
    }
    if expression.len() > MAX_EXPRESSION_BYTES {
        return Err(FormulaError::TooLong);
    }
    let tokens = tokenize(expression, &lookup)?;
    if tokens.is_empty() || tokens.len() > MAX_TOKENS {
        return Err(FormulaError::TooLong);
    }
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
    };
    let value = parser.expression()?;
    if parser.position != tokens.len() {
        return Err(FormulaError::Unexpected);
    }
    if !value.is_finite() {
        return Err(FormulaError::NotFinite);
    }
    Ok(value)
}

fn parse_numeric(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    let normalized = if raw.contains(',') && !raw.contains('.') {
        raw.replace(',', ".")
    } else {
        raw.to_owned()
    };
    normalized
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn tokenize(
    expression: &str,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<Vec<Token>, FormulaError> {
    let chars: Vec<char> = expression.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if character.is_whitespace() {
            index += 1;
            continue;
        }
        match character {
            '+' => {
                tokens.push(Token::Plus);
                index += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                index += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                index += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                index += 1;
            }
            '(' => {
                tokens.push(Token::Left);
                index += 1;
            }
            ')' => {
                tokens.push(Token::Right);
                index += 1;
            }
            '$' | 'A'..='Z' | 'a'..='z' => {
                if character == '$' {
                    index += 1;
                }
                let column_start = index;
                while index < chars.len() && chars[index].is_ascii_alphabetic() {
                    index += 1;
                }
                if index == column_start {
                    return Err(FormulaError::BadReference);
                }
                let column: String = chars[column_start..index].iter().collect();
                if index < chars.len() && chars[index] == '$' {
                    index += 1;
                }
                let row_start = index;
                while index < chars.len() && chars[index].is_ascii_digit() {
                    index += 1;
                }
                if row_start == index {
                    return Err(FormulaError::BadReference);
                }
                let row: String = chars[row_start..index].iter().collect();
                let reference = format!("{column}{row}");
                let raw = lookup(&reference)
                    .ok_or_else(|| FormulaError::NonNumericCell(reference.clone()))?;
                let value = parse_numeric(&raw).ok_or(FormulaError::NonNumericCell(reference))?;
                tokens.push(Token::Number(value));
            }
            '0'..='9' | '.' => {
                let start = index;
                while index < chars.len()
                    && (chars[index].is_ascii_digit()
                        || chars[index] == '.'
                        || (chars[index] == ','
                            && index + 1 < chars.len()
                            && chars[index + 1].is_ascii_digit()))
                {
                    index += 1;
                }
                let raw: String = chars[start..index].iter().collect();
                let value = parse_numeric(&raw).ok_or(FormulaError::BadNumber)?;
                tokens.push(Token::Number(value));
            }
            _ => return Err(FormulaError::Unexpected),
        }
        if tokens.len() > MAX_TOKENS {
            return Err(FormulaError::TooLong);
        }
    }
    Ok(tokens)
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
}

impl Parser<'_> {
    fn expression(&mut self) -> Result<f64, FormulaError> {
        let mut value = self.term()?;
        loop {
            match self.peek() {
                Some(Token::Plus) => {
                    self.position += 1;
                    value += self.term()?;
                }
                Some(Token::Minus) => {
                    self.position += 1;
                    value -= self.term()?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn term(&mut self) -> Result<f64, FormulaError> {
        let mut value = self.factor()?;
        loop {
            match self.peek() {
                Some(Token::Star) => {
                    self.position += 1;
                    value *= self.factor()?;
                }
                Some(Token::Slash) => {
                    self.position += 1;
                    let divisor = self.factor()?;
                    if divisor == 0.0 {
                        return Err(FormulaError::DivideByZero);
                    }
                    value /= divisor;
                }
                _ => return Ok(value),
            }
        }
    }

    fn factor(&mut self) -> Result<f64, FormulaError> {
        match self.peek() {
            Some(Token::Number(value)) => {
                self.position += 1;
                Ok(value)
            }
            Some(Token::Left) => {
                self.position += 1;
                let value = self.expression()?;
                if self.peek() != Some(Token::Right) {
                    return Err(FormulaError::Unexpected);
                }
                self.position += 1;
                Ok(value)
            }
            Some(Token::Minus) => {
                self.position += 1;
                Ok(-self.factor()?)
            }
            Some(Token::Plus) => {
                self.position += 1;
                self.factor()
            }
            _ => Err(FormulaError::Unexpected),
        }
    }

    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.position).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(reference: &str) -> Option<String> {
        match reference {
            "A1" => Some("10".into()),
            "B1" => Some("2".into()),
            "C1" => Some("3,5".into()),
            _ => None,
        }
    }

    #[test]
    fn precedence_parentheses_and_decimal_comma() {
        assert_eq!(evaluate("=2+3*4", cells), Ok(14.0));
        assert_eq!(evaluate("=(2+3)*4", cells), Ok(20.0));
        assert_eq!(evaluate("=C1+1", cells), Ok(4.5));
    }

    #[test]
    fn cell_references_and_division() {
        assert_eq!(evaluate("=A1/B1", cells), Ok(5.0));
        assert_eq!(evaluate("=A1-B1", cells), Ok(8.0));
    }

    #[test]
    fn errors_are_explicit_and_do_not_return_partial_values() {
        assert_eq!(evaluate("=A1/0", cells), Err(FormulaError::DivideByZero));
        assert_eq!(
            evaluate("=Z9+1", cells),
            Err(FormulaError::NonNumericCell("Z9".into()))
        );
        assert_eq!(evaluate("=2+", cells), Err(FormulaError::Unexpected));
        assert_eq!(evaluate("=", cells), Err(FormulaError::Empty));
    }
    #[test]
    fn cell_references_map_to_zero_based_coordinates() {
        assert_eq!(parse_cell_reference("A1"), Some((0, 0)));
        assert_eq!(parse_cell_reference("B2"), Some((1, 1)));
        assert_eq!(parse_cell_reference("$AA$10"), Some((9, 26)));
        assert_eq!(parse_cell_reference("A0"), None);
    }
    #[test]
    fn full_precision_format_keeps_a_tiny_fraction() {
        assert_eq!(
            format_number(1.0 / 10_000_000_000_000_000.0, None),
            "0.0000000000000001"
        );
        assert_eq!(format_number(0.8, None), "0.8");
        assert_eq!(format_number(0.8, Some(0)), "1");
    }
    #[test]
    fn simple_formula_syntax_is_not_value_dependent() {
        assert!(is_simple_formula("=10/A1"));
        assert!(is_simple_formula("=1/0"));
        assert!(!is_simple_formula("=WEBSERVICE(\"x\")"));
    }

    #[test]
    fn inserting_row_shifts_formula_references() {
        assert_eq!(shift_references("=A2", 2, 1, 0, 0), "=A3");
        assert_eq!(shift_references("=$B$2+C1", 2, 1, 0, 0), "=$B$3+C1");
        assert_eq!(shift_references("=A1", 1, 1, 0, 0), "=A2");
    }
}

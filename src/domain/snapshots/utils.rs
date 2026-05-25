use crate::schema::{DefaultValue, IndexMethod, ReferenceAction, SortOrder};

pub fn parse_default_value(value: String) -> DefaultValue {
    let normalized = strip_wrapping_parentheses(value.trim());
    let lowered = normalized.to_ascii_lowercase();

    if lowered == "null" {
        DefaultValue::Null
    } else if is_quoted_literal(normalized) {
        DefaultValue::Literal(normalized[1..normalized.len() - 1].replace("''", "'"))
    } else if is_scalar_literal(normalized) {
        DefaultValue::Literal(normalized.to_string())
    } else {
        DefaultValue::Sql(normalized.to_string())
    }
}

pub fn normalize_default_expression(expr: &str) -> String {
    let normalized = strip_wrapping_parentheses(expr.trim());

    if let Some((value, cast)) = normalized.rsplit_once("::") {
        let value = value.trim();
        let cast = cast.trim();
        if !cast.is_empty() && (is_quoted_literal(value) || is_scalar_literal(value)) {
            return value.to_string();
        }
    }

    normalized.to_string()
}

pub fn parse_reference_action(action: Option<&str>) -> ReferenceAction {
    match action.map(str::to_ascii_uppercase).as_deref() {
        Some("R") | Some("RESTRICT") => ReferenceAction::Restrict,
        Some("C") | Some("CASCADE") => ReferenceAction::Cascade,
        Some("N") | Some("SET NULL") => ReferenceAction::SetNull,
        Some("D") | Some("SET DEFAULT") => ReferenceAction::SetDefault,
        _ => ReferenceAction::NoAction,
    }
}

pub fn parse_index_method(method: &str) -> IndexMethod {
    match method.to_ascii_lowercase().as_str() {
        "hash" => IndexMethod::Hash,
        "gist" => IndexMethod::Gist,
        "spgist" => IndexMethod::SpGist,
        "gin" => IndexMethod::Gin,
        "brin" => IndexMethod::Brin,
        _ => IndexMethod::BTree,
    }
}

pub fn parse_sort_order(order: Option<&str>) -> Option<SortOrder> {
    match order.map(str::to_ascii_uppercase).as_deref() {
        Some("A") | Some("ASC") => Some(SortOrder::Asc),
        Some("D") | Some("DESC") => Some(SortOrder::Desc),
        _ => None,
    }
}

pub fn push_unique(values: &mut Vec<String>, value: Option<&String>) {
    if let Some(value) = value
        && !values.contains(value)
    {
        values.push(value.clone());
    }
}

pub fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

pub fn strip_wrapping_parentheses(value: &str) -> &str {
    let mut result = value;
    while result.starts_with('(') && result.ends_with(')') {
        let Some((inner, tail)) = take_parenthesized(result) else {
            break;
        };
        if !tail.trim().is_empty() {
            break;
        }
        result = inner.trim();
    }
    result
}

pub fn take_parenthesized(value: &str) -> Option<(&str, &str)> {
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut start = None;
    let mut chars = value.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        if ch == '\'' {
            if in_string && matches!(chars.peek(), Some((_, '\''))) {
                chars.next();
                continue;
            }
            in_string = !in_string;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' => {
                if depth == 0 {
                    start = Some(idx + ch.len_utf8());
                }
                depth += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&value[start?..idx], &value[idx + ch.len_utf8()..]));
                }
            }
            _ => {}
        }
    }

    None
}

pub fn is_scalar_literal(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower == "true"
        || lower == "false"
        || value.parse::<i64>().is_ok()
        || value.parse::<f64>().is_ok()
}

pub fn is_quoted_literal(value: &str) -> bool {
    value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'')
}

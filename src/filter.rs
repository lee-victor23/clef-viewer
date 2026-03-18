use evalexpr::*;
use serde_json::Value as JsonValue;
use std::collections::HashSet;

/// Separator used in evalexpr identifiers to represent dot-access.
/// The user types `Foo.Bar`; we rewrite it to `Foo__Bar` before compilation.
const DOT_SEP: &str = "__";

/// Compiled property filter expression, cached for reuse across log records.
pub struct PropertyFilter {
    tree: Node,
}

impl PropertyFilter {
    pub fn compile(expr: &str) -> Result<Self, EvalexprError> {
        let rewritten = rewrite_dots(expr);
        let tree = build_operator_tree(&rewritten)?;
        Ok(Self { tree })
    }

    pub fn matches(&self, raw: &JsonValue) -> bool {
        let obj = match raw.as_object() {
            Some(o) => o,
            None => return false,
        };

        let mut context = HashMapContext::new();
        let mut field_names: HashSet<String> = HashSet::new();

        for (k, v) in obj {
            if k == "@x" {
                field_names.insert("Exception".into());
                let _ = context.set_value("Exception".into(), json_to_evalexpr(v));
                continue;
            }
            if k == "@m" {
                field_names.insert("@m".into());
                let _ = context.set_value("@m".into(), json_to_evalexpr(v));
                continue;
            }
            if k == "@t" || k == "@l" || k == "@mt" {
                continue;
            }
            // Strip leading '@' for property names (e.g. @rs -> rs)
            let base_key = if k.starts_with('@') { &k[1..] } else { k.as_str() };
            field_names.insert(base_key.to_string());
            let _ = context.set_value(base_key.to_string(), json_to_evalexpr(v));
            // Flatten nested objects with double-underscore separator
            if let JsonValue::Object(inner) = v {
                flatten_object(inner, base_key, &mut context, &mut field_names);
            }
        }

        // Has("FieldName") → true if property exists
        // Accepts both dot notation ("Foo.Bar") and internal notation ("Foo__Bar")
        let fields = field_names;
        let _ = context.set_function(
            "Has".into(),
            Function::new(move |arg| match arg {
                Value::String(name) => {
                    let check = name.replace('.', DOT_SEP);
                    Ok(Value::Boolean(fields.contains(&check) || fields.contains(name)))
                }
                _ => Err(EvalexprError::expected_string(arg.clone())),
            }),
        );

        let _ = context.set_function(
            "Contains".into(),
            Function::new(|arg| str_pair_fn(arg, |h, n| h.contains(n))),
        );

        let _ = context.set_function(
            "StartsWith".into(),
            Function::new(|arg| str_pair_fn(arg, |h, n| h.starts_with(n))),
        );

        let _ = context.set_function(
            "EndsWith".into(),
            Function::new(|arg| str_pair_fn(arg, |h, n| h.ends_with(n))),
        );

        match self.tree.eval_with_context(&context) {
            Ok(Value::Boolean(b)) => b,
            Ok(_) => false, // non-boolean result treated as no match
            Err(_) => false,
        }
    }
}

/// Rewrite `Ident.Ident` sequences to `Ident__Ident` so evalexpr treats them
/// as a single variable name. Does not touch dots inside string literals or
/// numeric literals (e.g. `3.14`).
fn rewrite_dots(expr: &str) -> String {
    let mut out = String::with_capacity(expr.len());
    let chars: Vec<char> = expr.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        let c = chars[i];
        // Skip string literals
        if c == '"' {
            out.push(c);
            i += 1;
            while i < len {
                out.push(chars[i]);
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                    out.push(chars[i]);
                } else if chars[i] == '"' {
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        // Check for identifier.identifier pattern
        if c == '.' && i > 0 && i + 1 < len {
            let before = chars[i - 1];
            let after = chars[i + 1];
            let before_is_ident = before.is_alphanumeric() || before == '_';
            let after_is_ident = after.is_alphabetic() || after == '_';
            if before_is_ident && after_is_ident {
                // Check that the char before the dot is not purely a digit sequence
                // (to avoid rewriting 3.14 -> 3__14)
                let mut j = out.len();
                let out_bytes = out.as_bytes();
                let mut all_digits = true;
                while j > 0 {
                    j -= 1;
                    let b = out_bytes[j];
                    if b.is_ascii_digit() {
                        continue;
                    } else if b.is_ascii_alphabetic() || b == b'_' {
                        all_digits = false;
                        break;
                    } else {
                        break;
                    }
                }
                if !all_digits {
                    out.push_str(DOT_SEP);
                    i += 1;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn str_pair_fn(
    arg: &Value,
    f: impl Fn(&str, &str) -> bool,
) -> EvalexprResult<Value> {
    match arg {
        Value::Tuple(args) if args.len() == 2 => match (&args[0], &args[1]) {
            (Value::String(a), Value::String(b)) => Ok(Value::Boolean(f(a, b))),
            _ => Ok(Value::Boolean(false)),
        },
        _ => Err(EvalexprError::expected_tuple(arg.clone())),
    }
}

fn json_to_evalexpr(v: &JsonValue) -> Value {
    match v {
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Empty
            }
        }
        JsonValue::Bool(b) => Value::Boolean(*b),
        JsonValue::Null => Value::Empty,
        other => Value::String(other.to_string()),
    }
}

/// Flatten a nested JSON object into underscore-separated keys in the evalexpr context.
/// For objects with `$type`, also registers `TypeName__field` aliases.
fn flatten_object(
    obj: &serde_json::Map<String, JsonValue>,
    prefix: &str,
    context: &mut HashMapContext,
    field_names: &mut HashSet<String>,
) {
    let type_prefix = obj.get("$type").and_then(|v| v.as_str()).map(|s| s.to_string());

    for (k, v) in obj {
        if k == "$type" {
            continue;
        }
        // parent__field (e.g. rs__Amount)
        let key = format!("{}{}{}", prefix, DOT_SEP, k);
        field_names.insert(key.clone());
        let _ = context.set_value(key.clone(), json_to_evalexpr(v));

        // TypeName__field alias (e.g. RazerpayStatus__Amount)
        if let Some(ref tp) = type_prefix {
            let alias = format!("{}{}{}", tp, DOT_SEP, k);
            if alias != key {
                field_names.insert(alias.clone());
                let _ = context.set_value(alias.clone(), json_to_evalexpr(v));
            }
        }

        // Recurse into nested objects
        if let JsonValue::Object(inner) = v {
            flatten_object(inner, &key, context, field_names);
            if let Some(ref tp) = type_prefix {
                let alias = format!("{}{}{}", tp, DOT_SEP, k);
                flatten_object(inner, &alias, context, field_names);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_dots_basic() {
        assert_eq!(rewrite_dots("Foo.Bar >= 0"), "Foo__Bar >= 0");
        assert_eq!(rewrite_dots("a.b.c == 1"), "a__b__c == 1");
    }

    #[test]
    fn test_rewrite_dots_preserves_floats() {
        assert_eq!(rewrite_dots("x > 3.14"), "x > 3.14");
        assert_eq!(rewrite_dots("Foo.Bar > 3.14"), "Foo__Bar > 3.14");
    }

    #[test]
    fn test_rewrite_dots_preserves_strings() {
        assert_eq!(rewrite_dots(r#"x == "a.b""#), r#"x == "a.b""#);
    }

    #[test]
    fn test_nested_object_filter() {
        let raw: JsonValue = serde_json::json!({
            "@t": "2026-01-01T00:00:00Z",
            "@l": "Information",
            "@rs": {
                "$type": "RazerpayStatus",
                "Amount": 666.26,
                "Status": "00"
            }
        });
        let pf = PropertyFilter::compile("RazerpayStatus.Amount >= 0").unwrap();
        assert!(pf.matches(&raw));

        let pf2 = PropertyFilter::compile("RazerpayStatus.Amount > 1000").unwrap();
        assert!(!pf2.matches(&raw));

        // Also accessible via parent key
        let pf3 = PropertyFilter::compile("rs.Amount >= 0").unwrap();
        assert!(pf3.matches(&raw));

        // String field
        let pf4 = PropertyFilter::compile(r#"RazerpayStatus.Status == "00""#).unwrap();
        assert!(pf4.matches(&raw));
    }

    #[test]
    fn test_has_with_dot_notation() {
        let raw: JsonValue = serde_json::json!({
            "@t": "2026-01-01T00:00:00Z",
            "@rs": {
                "$type": "RazerpayStatus",
                "Amount": 100
            }
        });
        let pf = PropertyFilter::compile(r#"Has("RazerpayStatus.Amount")"#).unwrap();
        assert!(pf.matches(&raw));
    }
}

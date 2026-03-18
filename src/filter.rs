use evalexpr::*;
use serde_json::Value as JsonValue;
use std::collections::HashSet;

/// Compiled property filter expression, cached for reuse across log records.
pub struct PropertyFilter {
    tree: Node,
}

impl PropertyFilter {
    pub fn compile(expr: &str) -> Result<Self, EvalexprError> {
        let tree = build_operator_tree(expr)?;
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
            field_names.insert(k.clone());
            let _ = context.set_value(k.clone(), json_to_evalexpr(v));
        }

        // Has("FieldName") → true if property exists
        let fields = field_names;
        let _ = context.set_function(
            "Has".into(),
            Function::new(move |arg| match arg {
                Value::String(name) => Ok(Value::Boolean(fields.contains(name))),
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

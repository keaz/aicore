use anyhow::{anyhow, bail};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FieldSpec {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VariantSpec {
    pub name: String,
    pub payload: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ParamSpec {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MatchArmSpec {
    pub pattern: String,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FnScaffoldOptions {
    pub name: String,
    pub params: Vec<ParamSpec>,
    pub return_type: String,
    pub effects: Vec<String>,
    pub capabilities: Vec<String>,
    pub requires: Option<String>,
    pub ensures: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TestScaffoldOptions {
    pub target_function: String,
    pub include_run_pass: bool,
    pub include_compile_fail: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScaffoldOutput {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub content: String,
}

pub fn parse_struct_fields(specs: &[String]) -> anyhow::Result<Vec<FieldSpec>> {
    specs
        .iter()
        .map(|spec| {
            let (name, ty) = parse_named_type(spec)?;
            Ok(FieldSpec { name, ty })
        })
        .collect()
}

pub fn parse_enum_variants(specs: &[String]) -> anyhow::Result<Vec<VariantSpec>> {
    specs.iter().map(|spec| parse_variant_spec(spec)).collect()
}

pub fn parse_params(specs: &[String]) -> anyhow::Result<Vec<ParamSpec>> {
    specs
        .iter()
        .map(|spec| {
            let (name, ty) = parse_named_type(spec)?;
            Ok(ParamSpec { name, ty })
        })
        .collect()
}

pub fn parse_match_arms(specs: &[String]) -> anyhow::Result<Vec<MatchArmSpec>> {
    specs
        .iter()
        .map(|spec| {
            let mut parts = spec.splitn(2, "=>");
            let pattern = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("invalid arm `{spec}`: missing pattern"))?
                .to_string();
            let body = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
            Ok(MatchArmSpec { pattern, body })
        })
        .collect()
}

pub fn parse_inline_items(raw_tokens: &[String]) -> anyhow::Result<Vec<String>> {
    if raw_tokens.is_empty() {
        return Ok(Vec::new());
    }

    let joined = raw_tokens.join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let content = if trimmed.starts_with('{') {
        if !trimmed.ends_with('}') {
            bail!("inline list must end with `}}`");
        }
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    Ok(content
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect())
}

pub fn scaffold_struct(
    name: &str,
    fields: &[FieldSpec],
    invariant: Option<&str>,
) -> ScaffoldOutput {
    let mut lines = Vec::new();
    lines.push(format!("struct {name} {{"));
    if fields.is_empty() {
        lines.push("    // TODO: add fields".to_string());
    } else {
        for field in fields {
            lines.push(format!("    {}: {},", field.name, field.ty));
        }
    }
    lines.push("}".to_string());
    if let Some(invariant) = invariant {
        lines.push(format!("invariant {}", invariant.trim()));
    }

    ScaffoldOutput {
        kind: "struct".to_string(),
        name: Some(name.to_string()),
        content: lines.join("\n"),
    }
}

pub fn scaffold_enum(name: &str, variants: &[VariantSpec]) -> ScaffoldOutput {
    let mut lines = Vec::new();
    lines.push(format!("enum {name} {{"));
    if variants.is_empty() {
        lines.push("    // TODO: add variants".to_string());
    } else {
        for variant in variants {
            if let Some(payload) = &variant.payload {
                lines.push(format!("    {}({}),", variant.name, payload));
            } else {
                lines.push(format!("    {},", variant.name));
            }
        }
    }
    lines.push("}".to_string());

    ScaffoldOutput {
        kind: "enum".to_string(),
        name: Some(name.to_string()),
        content: lines.join("\n"),
    }
}

pub fn scaffold_function(options: &FnScaffoldOptions) -> ScaffoldOutput {
    let default_body = default_expression_for_return_type(&options.return_type);
    scaffold_function_with_body(options, &format!("// TODO: implement\n{default_body}"))
}

pub fn scaffold_function_with_body(options: &FnScaffoldOptions, body: &str) -> ScaffoldOutput {
    let params = options
        .params
        .iter()
        .map(|param| format!("{}: {}", param.name, param.ty))
        .collect::<Vec<_>>()
        .join(", ");

    let mut signature = format!(
        "fn {}({params}) -> {}",
        options.name,
        options.return_type.trim()
    );

    if !options.effects.is_empty() {
        signature.push_str(" effects { ");
        signature.push_str(&options.effects.join(", "));
        signature.push_str(" }");
    }

    if !options.capabilities.is_empty() {
        signature.push_str(" capabilities { ");
        signature.push_str(&options.capabilities.join(", "));
        signature.push_str(" }");
    }

    if let Some(requires) = &options.requires {
        signature.push_str(" requires ");
        signature.push_str(requires.trim());
    }

    if let Some(ensures) = &options.ensures {
        signature.push_str(" ensures ");
        signature.push_str(ensures.trim());
    }

    let body = indent_block(body.trim_end());
    let content = if body.is_empty() {
        format!("{signature} {{\n}}")
    } else {
        format!("{signature} {{\n{body}\n}}")
    };

    ScaffoldOutput {
        kind: "fn".to_string(),
        name: Some(options.name.clone()),
        content,
    }
}

pub fn scaffold_match(
    expr: &str,
    mut arms: Vec<MatchArmSpec>,
    exhaustive: bool,
) -> anyhow::Result<ScaffoldOutput> {
    if exhaustive && arms.is_empty() {
        bail!("--exhaustive requires at least one --arm pattern");
    }

    if !exhaustive && !arms.iter().any(|arm| arm.pattern == "_") {
        arms.push(MatchArmSpec {
            pattern: "_".to_string(),
            body: Some("todo()".to_string()),
        });
    }

    let mut lines = Vec::new();
    lines.push(format!("match {} {{", expr.trim()));
    for arm in arms {
        let body = arm.body.unwrap_or_else(|| "todo()".to_string());
        lines.push(format!("    {} => {},", arm.pattern, body));
    }
    lines.push("}".to_string());

    Ok(ScaffoldOutput {
        kind: "match".to_string(),
        name: None,
        content: lines.join("\n"),
    })
}

pub fn scaffold_test(options: &TestScaffoldOptions) -> ScaffoldOutput {
    let mut blocks = Vec::new();
    let base_name = sanitize_identifier(&options.target_function);

    if options.include_run_pass {
        blocks.push(format!(
            "#[test]\nfn test_{}_run_pass() -> () {{\n    // TODO: provide valid arguments for {}\n    let _out = {}(/* args */);\n    assert(true);\n}}",
            base_name, options.target_function, options.target_function
        ));
    }

    if options.include_compile_fail {
        blocks.push(format!(
            "// compile-fail fixture template:\n// #[test]\n// fn test_{}_compile_fail() -> () {{\n//     // TODO: intentionally pass invalid arguments to {} and assert diagnostics.\n// }}",
            base_name, options.target_function
        ));
    }

    if blocks.is_empty() {
        blocks.push("// No test variants selected".to_string());
    }

    ScaffoldOutput {
        kind: "test".to_string(),
        name: Some(options.target_function.clone()),
        content: blocks.join("\n\n"),
    }
}

fn parse_named_type(spec: &str) -> anyhow::Result<(String, String)> {
    let mut parts = spec.splitn(2, ':');
    let name = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("invalid spec `{spec}`: expected name:type"))?;
    let ty = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("invalid spec `{spec}`: expected name:type"))?;

    Ok((name.to_string(), ty.to_string()))
}

fn parse_variant_spec(spec: &str) -> anyhow::Result<VariantSpec> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        bail!("empty enum variant spec");
    }

    if let Some((name, payload)) = trimmed.split_once(':') {
        let name = name.trim();
        let payload = payload.trim();
        if name.is_empty() || payload.is_empty() {
            bail!("invalid variant spec `{spec}`: expected Name:Type");
        }
        return Ok(VariantSpec {
            name: name.to_string(),
            payload: Some(payload.to_string()),
        });
    }

    if let Some(open_idx) = trimmed.find('(') {
        if !trimmed.ends_with(')') {
            bail!("invalid variant spec `{spec}`: missing closing `)`");
        }
        let name = trimmed[..open_idx].trim();
        let payload = trimmed[open_idx + 1..trimmed.len() - 1].trim();
        if name.is_empty() || payload.is_empty() {
            bail!("invalid variant spec `{spec}`");
        }
        return Ok(VariantSpec {
            name: name.to_string(),
            payload: Some(payload.to_string()),
        });
    }

    Ok(VariantSpec {
        name: trimmed.to_string(),
        payload: None,
    })
}

fn default_expression_for_return_type(return_type: &str) -> String {
    let compact = return_type.split_whitespace().collect::<String>();
    match compact.as_str() {
        "Unit" | "()" => "()".to_string(),
        "Bool" => "false".to_string(),
        "String" => "\"\"".to_string(),
        "Int" | "UInt" | "USize" | "I8" | "I16" | "I32" | "I64" | "I128" | "U8" | "U16" | "U32"
        | "U64" | "U128" | "Float32" | "Float64" => "0".to_string(),
        other if other.starts_with("Option[") => "None".to_string(),
        other if other.starts_with("Result[") => {
            if let Some(ok_ty) = top_level_generic_arguments(other).first() {
                format!("Ok({})", default_expression_for_return_type(ok_ty))
            } else {
                "Ok(())".to_string()
            }
        }
        _ => "todo()".to_string(),
    }
}

fn indent_block(body: &str) -> String {
    body.lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("    {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn top_level_generic_arguments(compact: &str) -> Vec<String> {
    let Some(open) = compact.find('[') else {
        return Vec::new();
    };
    let Some(close) = compact.rfind(']') else {
        return Vec::new();
    };
    if close <= open + 1 {
        return Vec::new();
    }

    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = open + 1;
    let chars = compact.char_indices().collect::<Vec<_>>();
    for (idx, ch) in chars {
        if idx <= open {
            continue;
        }
        if idx >= close {
            break;
        }
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let value = compact[start..idx].trim();
                if !value.is_empty() {
                    parts.push(value.to_string());
                }
                start = idx + 1;
            }
            _ => {}
        }
    }

    let tail = compact[start..close].trim();
    if !tail.is_empty() {
        parts.push(tail.to_string());
    }

    parts
}

fn sanitize_identifier(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }

    if out.is_empty() {
        "scaffold".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_inline_items, parse_struct_fields, scaffold_function, scaffold_match,
        scaffold_struct, FieldSpec, FnScaffoldOptions, MatchArmSpec,
    };

    #[test]
    fn parse_inline_items_supports_brace_style_input() {
        let raw = vec![
            "{".to_string(),
            "name: String,".to_string(),
            "age: Int".to_string(),
            "}".to_string(),
        ];
        let parsed = parse_inline_items(&raw).expect("parse inline list");
        assert_eq!(parsed, vec!["name: String", "age: Int"]);
    }

    #[test]
    fn struct_scaffold_includes_invariant_when_provided() {
        let output = scaffold_struct(
            "User",
            &[FieldSpec {
                name: "age".to_string(),
                ty: "Int".to_string(),
            }],
            Some("age >= 0"),
        );
        assert!(output.content.contains("struct User"));
        assert!(output.content.contains("invariant age >= 0"));
    }

    #[test]
    fn function_scaffold_emits_effects_and_contract_stubs() {
        let output = scaffold_function(&FnScaffoldOptions {
            name: "process_user".to_string(),
            params: parse_struct_fields(&["u: User".to_string()])
                .expect("parse params")
                .into_iter()
                .map(|field| super::ParamSpec {
                    name: field.name,
                    ty: field.ty,
                })
                .collect(),
            return_type: "Result[Int, AppError]".to_string(),
            effects: vec!["io".to_string()],
            capabilities: Vec::new(),
            requires: Some("true".to_string()),
            ensures: Some("true".to_string()),
        });
        assert!(output.content.contains("effects { io }"));
        assert!(output.content.contains("requires true"));
        assert!(output.content.contains("ensures true"));
    }

    #[test]
    fn match_scaffold_adds_wildcard_when_not_exhaustive() {
        let output = scaffold_match(
            "value",
            vec![MatchArmSpec {
                pattern: "Some(v)".to_string(),
                body: None,
            }],
            false,
        )
        .expect("scaffold match");
        assert!(output.content.contains("Some(v) => todo()"));
        assert!(output.content.contains("_ => todo()"));
    }
}

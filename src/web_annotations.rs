use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::ast::{
    self, Attribute, AttributeArg, AttributeValue, AttributeValueKind, BoolLiteral, TypeExpr,
    TypeKind,
};
use crate::diagnostics::{Diagnostic, Severity};
use crate::machine_paths;
use crate::package_loader::{load_entry_with_options, LoadOptions};
use crate::parser;

const GENERATED_FILE: &str = "<aicore-web-generated>";
const GENERATED_APP_FN: &str = "build_annotated_app";
const EFFECTS: &str = "effects { io, fs, net, env, proc, time, rand, concurrency }";
const CAPABILITIES: &str = "capabilities { io, fs, net, env, proc, time, rand, concurrency }";

#[derive(Debug, Clone, Default)]
pub struct WebGeneration {
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
    pub route_count: usize,
    pub filter_count: usize,
    pub validator_count: usize,
}

#[derive(Debug, Clone)]
struct RouteSpec {
    method: String,
    path: String,
    function_name: String,
    wrapper_name: String,
    attr_span: crate::span::Span,
}

#[derive(Debug, Clone)]
struct FilterSpec {
    order: i64,
    struct_name: String,
    attr_span: crate::span::Span,
}

#[derive(Debug, Clone)]
struct ValidatorSpec {
    struct_name: String,
    fields: Vec<ValidatedField>,
}

#[derive(Debug, Clone)]
struct ValidatedField {
    name: String,
    ty: String,
    rules: Vec<ValidationRule>,
    attr_span: crate::span::Span,
}

#[derive(Debug, Clone)]
enum ValidationRule {
    Required,
    MinLength(i64),
    MaxLength(i64),
    Min(i64),
    Max(i64),
    OneOf(String),
    Custom(String),
}

pub fn generate_for_path(input: &Path, offline: bool) -> anyhow::Result<WebGeneration> {
    let loaded = load_entry_with_options(input, LoadOptions { offline })?;
    let Some(program) = loaded.program else {
        return Ok(WebGeneration {
            diagnostics: loaded.diagnostics,
            ..WebGeneration::default()
        });
    };
    let file = entry_module_file(&program, &loaded.module_files)
        .unwrap_or_else(|| machine_paths::canonical_machine_path(input));
    let mut generation = generate(&program, &loaded.item_modules, &file);
    generation.diagnostics.extend(loaded.diagnostics);
    Ok(generation)
}

pub fn augment_program(
    program: &mut ast::Program,
    item_modules: &mut Vec<Option<Vec<String>>>,
    file: &str,
) -> Vec<Diagnostic> {
    let generation = generate(program, item_modules, file);
    if generation.source.trim().is_empty()
        || generation
            .diagnostics
            .iter()
            .any(|diag| matches!(diag.severity, Severity::Error))
    {
        return generation.diagnostics;
    }

    let (generated, parse_diags) = parser::parse(&generation.source, GENERATED_FILE);
    let mut diagnostics = generation.diagnostics;
    diagnostics.extend(parse_diags);
    if diagnostics
        .iter()
        .any(|diag| matches!(diag.severity, Severity::Error))
    {
        return diagnostics;
    }

    if let Some(generated_program) = generated {
        let module = program.module.as_ref().map(|decl| decl.path.clone());
        for item in generated_program.items {
            program.items.push(item);
            item_modules.push(module.clone());
        }
    }

    diagnostics
}

pub fn generate(
    program: &ast::Program,
    item_modules: &[Option<Vec<String>>],
    file: &str,
) -> WebGeneration {
    let entry_module = program.module.as_ref().map(|decl| decl.path.clone());
    let mut diagnostics = Vec::new();
    let mut routes = Vec::new();
    let mut filters = Vec::new();
    let mut validators = Vec::new();

    for (index, item) in program.items.iter().enumerate() {
        if item_modules.get(index).cloned().unwrap_or(None) != entry_module {
            continue;
        }
        match item {
            ast::Item::Function(function) => {
                collect_routes(file, function, routes.len(), &mut routes, &mut diagnostics);
            }
            ast::Item::Struct(def) => {
                collect_filter(file, def, &mut filters, &mut diagnostics);
                collect_validator(file, def, &mut validators, &mut diagnostics);
            }
            _ => {}
        }
    }

    validate_duplicate_routes(file, &routes, &mut diagnostics);
    validate_duplicate_filters(file, &filters, &mut diagnostics);

    let source = if diagnostics
        .iter()
        .any(|diag| matches!(diag.severity, Severity::Error))
    {
        String::new()
    } else {
        render_generated_source(&routes, &filters, &validators)
    };

    WebGeneration {
        source,
        diagnostics,
        route_count: routes.len(),
        filter_count: filters.len(),
        validator_count: validators.len(),
    }
}

pub fn entry_module_file(
    program: &ast::Program,
    module_files: &BTreeMap<String, String>,
) -> Option<String> {
    let module = program
        .module
        .as_ref()
        .map(|decl| decl.path.join("."))
        .unwrap_or_else(|| "<root>".to_string());
    module_files.get(&module).cloned()
}

fn collect_routes(
    file: &str,
    function: &ast::Function,
    route_offset: usize,
    routes: &mut Vec<RouteSpec>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let route_attrs = function
        .attrs
        .iter()
        .filter_map(route_attr)
        .collect::<Vec<_>>();
    if route_attrs.is_empty() {
        return;
    }

    if !function.is_async {
        diagnostics.push(
            Diagnostic::error(
                "E1096",
                format!(
                    "annotated route handler `{}` must be declared `async fn`",
                    function.name
                ),
                file,
                function.span,
            )
            .with_help("route attributes lower to async Handler implementations"),
        );
    }

    if function.params.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "E1096",
                format!(
                    "annotated route handler `{}` must accept exactly one RequestContext parameter",
                    function.name
                ),
                file,
                function.span,
            )
            .with_help("use `async fn handler(req: RequestContext) -> ResponseEntity`"),
        );
    }

    if function.params.len() == 1 && type_repr(&function.params[0].ty) != "RequestContext" {
        diagnostics.push(
            Diagnostic::error(
                "E1096",
                format!(
                    "annotated route handler `{}` parameter must be `RequestContext`",
                    function.name
                ),
                file,
                function.params[0].span,
            )
            .with_help("use `async fn handler(req: RequestContext) -> ResponseEntity`"),
        );
    }

    if type_repr(&function.ret_type) != "ResponseEntity" {
        diagnostics.push(
            Diagnostic::error(
                "E1096",
                format!(
                    "annotated route handler `{}` must return `ResponseEntity`",
                    function.name
                ),
                file,
                function.ret_type.span,
            )
            .with_help("use `async fn handler(req: RequestContext) -> ResponseEntity`"),
        );
    }

    if !function.generics.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "E1096",
                format!(
                    "annotated route handler `{}` cannot declare generic parameters",
                    function.name
                ),
                file,
                function.span,
            )
            .with_help("register generic handlers manually with `web.add_route`"),
        );
    }

    if route_attrs.is_empty()
        || !function.is_async
        || function.params.len() != 1
        || type_repr(&function.params[0].ty) != "RequestContext"
        || type_repr(&function.ret_type) != "ResponseEntity"
        || !function.generics.is_empty()
    {
        return;
    }

    for (local_index, (method, path, attr_span)) in route_attrs.into_iter().enumerate() {
        routes.push(RouteSpec {
            method,
            path,
            function_name: function.name.clone(),
            wrapper_name: format!(
                "AicoreWebRoute_{}_{}",
                route_offset + local_index,
                sanitize_ident(&function.name)
            ),
            attr_span,
        });
    }
}

fn collect_filter(
    file: &str,
    def: &ast::StructDef,
    filters: &mut Vec<FilterSpec>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some((order, attr_span)) = def.attrs.iter().find_map(filter_attr) else {
        return;
    };
    if !def.fields.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "E1096",
                format!(
                    "annotated filter `{}` must be a zero-field struct so it can be generated deterministically",
                    def.name
                ),
                file,
                def.span,
            )
            .with_help("use `struct MyFilter {}` or register stateful filters manually"),
        );
        return;
    }
    if !def.generics.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "E1096",
                format!(
                    "annotated filter `{}` cannot declare generic parameters",
                    def.name
                ),
                file,
                def.span,
            )
            .with_help("register generic filters manually with `web.add_filter`"),
        );
        return;
    }
    filters.push(FilterSpec {
        order,
        struct_name: def.name.clone(),
        attr_span,
    });
}

fn collect_validator(
    file: &str,
    def: &ast::StructDef,
    validators: &mut Vec<ValidatorSpec>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut fields = Vec::new();
    for field in &def.fields {
        let mut rules = Vec::new();
        let mut attr_span = field.span;
        for attr in field.attrs.iter().filter(|attr| attr.name == "validate") {
            attr_span = attr.span;
            rules.extend(validation_rules(file, attr, &field.name, diagnostics));
        }
        if !rules.is_empty() {
            fields.push(ValidatedField {
                name: field.name.clone(),
                ty: type_repr(&field.ty),
                rules,
                attr_span,
            });
        }
    }

    if fields.is_empty() {
        return;
    }

    if !def.generics.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "E1096",
                format!(
                    "annotated validator `{}` cannot declare generic parameters",
                    def.name
                ),
                file,
                def.span,
            )
            .with_help("write generic validation implementations manually"),
        );
        return;
    }

    for field in &fields {
        validate_rule_type_support(file, field, diagnostics);
    }

    validators.push(ValidatorSpec {
        struct_name: def.name.clone(),
        fields,
    });
}

fn validate_rule_type_support(
    file: &str,
    field: &ValidatedField,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for rule in &field.rules {
        let supported = match rule {
            ValidationRule::Required
            | ValidationRule::MinLength(_)
            | ValidationRule::MaxLength(_)
            | ValidationRule::OneOf(_) => field.ty == "String",
            ValidationRule::Min(_) | ValidationRule::Max(_) => field.ty == "Int",
            ValidationRule::Custom(name) => {
                if !is_identifier_path(name) {
                    diagnostics.push(
                        Diagnostic::error(
                            "E1097",
                            format!(
                                "custom validation hook `{}` on field `{}` is not a valid function path",
                                name, field.name
                            ),
                            file,
                            field.attr_span,
                        )
                        .with_help("use an identifier or dotted identifier path like `validators.check_name`"),
                    );
                }
                true
            }
        };
        if !supported {
            diagnostics.push(
                Diagnostic::error(
                    "E1097",
                    format!(
                        "validation rule on field `{}` is not supported for type `{}`",
                        field.name, field.ty
                    ),
                    file,
                    field.attr_span,
                )
                .with_help("use string rules on String fields and numeric rules on Int fields"),
            );
        }
    }
}

fn validate_duplicate_routes(file: &str, routes: &[RouteSpec], diagnostics: &mut Vec<Diagnostic>) {
    let mut seen = BTreeMap::<(String, String), &RouteSpec>::new();
    for route in routes {
        let key = (route.method.clone(), route.path.clone());
        if let Some(first) = seen.get(&key) {
            diagnostics.push(
                Diagnostic::error(
                    "E1096",
                    format!(
                        "duplicate annotated route `{}` `{}` on `{}`",
                        route.method, route.path, route.function_name
                    ),
                    file,
                    route.attr_span,
                )
                .with_help(format!(
                    "first annotated route was declared on `{}`",
                    first.function_name
                )),
            );
        } else {
            seen.insert(key, route);
        }
    }
}

fn validate_duplicate_filters(
    file: &str,
    filters: &[FilterSpec],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen = BTreeMap::<i64, &FilterSpec>::new();
    for filter in filters {
        if let Some(first) = seen.get(&filter.order) {
            diagnostics.push(
                Diagnostic::error(
                    "E1096",
                    format!(
                        "duplicate annotated filter order `{}` on `{}`",
                        filter.order, filter.struct_name
                    ),
                    file,
                    filter.attr_span,
                )
                .with_help(format!(
                    "first annotated filter with this order was `{}`",
                    first.struct_name
                )),
            );
        } else {
            seen.insert(filter.order, filter);
        }
    }
}

fn render_generated_source(
    routes: &[RouteSpec],
    filters: &[FilterSpec],
    validators: &[ValidatorSpec],
) -> String {
    if routes.is_empty() && filters.is_empty() && validators.is_empty() {
        return String::new();
    }

    let mut source = String::new();
    let mut sorted_routes = routes.to_vec();
    sorted_routes.sort_by(|a, b| {
        a.method
            .cmp(&b.method)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.function_name.cmp(&b.function_name))
    });
    let mut sorted_filters = filters.to_vec();
    sorted_filters.sort_by(|a, b| {
        a.order
            .cmp(&b.order)
            .then_with(|| a.struct_name.cmp(&b.struct_name))
    });

    for route in &sorted_routes {
        source.push_str(&render_route_wrapper(route));
        source.push('\n');
    }

    for validator in validators {
        source.push_str(&render_validator(validator));
        source.push('\n');
    }

    if !sorted_routes.is_empty() || !sorted_filters.is_empty() {
        source.push_str(&render_app_builder(&sorted_filters, &sorted_routes));
    }

    source
}

fn render_route_wrapper(route: &RouteSpec) -> String {
    format!(
        "struct {wrapper} {{}}\n\nimpl Handler[{wrapper}] {{\n    async fn handle(self: {wrapper}, req: RequestContext) -> ResponseEntity {effects} {capabilities} {{\n        let _self = self;\n        await {handler}(req)\n    }}\n}}\n",
        wrapper = route.wrapper_name,
        handler = route.function_name,
        effects = EFFECTS,
        capabilities = CAPABILITIES,
    )
}

fn render_validator(spec: &ValidatorSpec) -> String {
    let fn_name = format!(
        "__aicore_web_validate_{}_with_mode",
        sanitize_ident(&spec.struct_name)
    );
    let mut source = String::new();
    source.push_str(&format!(
        "fn {fn_name}(value: {ty}, mode: ValidationMode) -> Result[{ty}, ValidationErrors] {{\n",
        fn_name = fn_name,
        ty = spec.struct_name,
    ));
    source.push_str("    let errors0 = validation.empty_errors();\n");
    let mut current = "errors0".to_string();
    let mut index = 1usize;
    for field in &spec.fields {
        for rule in &field.rules {
            let next = format!("errors{index}");
            source.push_str(&format!(
                "    let {next} = if validation.should_continue(mode, {current}) {{\n        {call}\n    }} else {{\n        {current}\n    }};\n",
                next = next,
                current = current,
                call = render_validation_rule_call(&current, field, rule),
            ));
            current = next;
            index += 1;
        }
    }
    source.push_str(&format!(
        "    if validation.has_errors({errors}) {{\n        Err({errors})\n    }} else {{\n        Ok(value)\n    }}\n}}\n\n",
        errors = current,
    ));
    source.push_str(&format!(
        "impl Validate[{ty}] {{\n    fn validate(self: {ty}) -> Result[{ty}, ValidationErrors] {{\n        {fn_name}(self, validation.collect_all_mode())\n    }}\n}}\n",
        ty = spec.struct_name,
        fn_name = fn_name,
    ));
    source
}

fn render_validation_rule_call(
    current: &str,
    field: &ValidatedField,
    rule: &ValidationRule,
) -> String {
    let field_expr = format!("value.{}", field.name);
    match rule {
        ValidationRule::Required => format!(
            "validation.required_string({current}, \"{field}\", {field_expr})",
            current = current,
            field = escape_string(&field.name),
            field_expr = field_expr,
        ),
        ValidationRule::MinLength(value) => format!(
            "validation.min_length({current}, \"{field}\", {field_expr}, {value})",
            current = current,
            field = escape_string(&field.name),
            field_expr = field_expr,
            value = value,
        ),
        ValidationRule::MaxLength(value) => format!(
            "validation.max_length({current}, \"{field}\", {field_expr}, {value})",
            current = current,
            field = escape_string(&field.name),
            field_expr = field_expr,
            value = value,
        ),
        ValidationRule::Min(value) => format!(
            "validation.min_int({current}, \"{field}\", {field_expr}, {value})",
            current = current,
            field = escape_string(&field.name),
            field_expr = field_expr,
            value = value,
        ),
        ValidationRule::Max(value) => format!(
            "validation.max_int({current}, \"{field}\", {field_expr}, {value})",
            current = current,
            field = escape_string(&field.name),
            field_expr = field_expr,
            value = value,
        ),
        ValidationRule::OneOf(value) => format!(
            "validation.one_of_string_list({current}, \"{field}\", {field_expr}, \"{allowed}\")",
            current = current,
            field = escape_string(&field.name),
            field_expr = field_expr,
            allowed = escape_string(value),
        ),
        ValidationRule::Custom(name) => format!(
            "{name}({current}, \"{field}\", {field_expr})",
            name = name,
            current = current,
            field = escape_string(&field.name),
            field_expr = field_expr,
        ),
    }
}

fn render_app_builder(filters: &[FilterSpec], routes: &[RouteSpec]) -> String {
    let mut steps = Vec::new();
    for filter in filters {
        steps.push(format!(
            "web.add_filter({app}, {order}, {name} {{}})",
            app = "{app}",
            order = filter.order,
            name = filter.struct_name,
        ));
    }
    for route in routes {
        steps.push(format!(
            "web.add_route({app}, \"{method}\", \"{path}\", {wrapper} {{}})",
            app = "{app}",
            method = route.method,
            path = escape_string(&route.path),
            wrapper = route.wrapper_name,
        ));
    }

    let body = render_registration_step(0, "app0", &steps);
    format!(
        "fn {name}() -> Result[App, FrameworkError] {{\n    match web.new_app() {{\n        Ok(app0) => {body},\n        Err(err) => Err(err),\n    }}\n}}\n",
        name = GENERATED_APP_FN,
        body = indent_block(&body, 8),
    )
}

fn render_registration_step(index: usize, current_app: &str, steps: &[String]) -> String {
    if index >= steps.len() {
        return format!("Ok({current_app})");
    }
    let next_app = format!("app{}", index + 1);
    let call = steps[index].replace("{app}", current_app);
    let ok = render_registration_step(index + 1, &next_app, steps);
    format!(
        "match {call} {{\n    Ok({next_app}) => {ok},\n    Err(err) => Err(err),\n}}",
        call = call,
        next_app = next_app,
        ok = indent_block(&ok, 4),
    )
}

fn route_attr(attr: &Attribute) -> Option<(String, String, crate::span::Span)> {
    let method = match attr.name.as_str() {
        "get" => "GET",
        "post" => "POST",
        "put" => "PUT",
        "patch" => "PATCH",
        "delete" => "DELETE",
        _ => return None,
    };
    positional_string(attr).map(|path| (method.to_string(), path, attr.span))
}

fn filter_attr(attr: &Attribute) -> Option<(i64, crate::span::Span)> {
    if attr.name != "filter" {
        return None;
    }
    named_int(attr, "order").map(|order| (order, attr.span))
}

fn validation_rules(
    file: &str,
    attr: &Attribute,
    field_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ValidationRule> {
    let mut rules = Vec::new();
    let mut seen = BTreeSet::<String>::new();
    for arg in &attr.args {
        match validation_rule(arg) {
            Some((key, rule)) => {
                if seen.insert(key) {
                    rules.push(rule);
                } else {
                    diagnostics.push(
                        Diagnostic::error(
                            "E1096",
                            format!("duplicate validation rule on field `{field_name}`"),
                            file,
                            attr.span,
                        )
                        .with_help("remove duplicate rules from the `#[validate(...)]` attribute"),
                    );
                }
            }
            None => diagnostics.push(
                Diagnostic::error(
                    "E1097",
                    format!("unsupported validation rule on field `{field_name}`"),
                    file,
                    attr.span,
                )
                .with_help(
                    "supported rules: required, min_length, max_length, min, max, one_of, custom",
                ),
            ),
        }
    }
    rules
}

fn validation_rule(arg: &AttributeArg) -> Option<(String, ValidationRule)> {
    match arg {
        AttributeArg::Positional(value) => match &value.kind {
            AttributeValueKind::Ident(name) if name == "required" => {
                Some(("required".to_string(), ValidationRule::Required))
            }
            _ => None,
        },
        AttributeArg::Named { name, value, .. } => match (name.as_str(), &value.kind) {
            ("required", AttributeValueKind::Bool(BoolLiteral::True)) => {
                Some(("required".to_string(), ValidationRule::Required))
            }
            ("min_length", AttributeValueKind::Int(value)) => {
                Some(("min_length".to_string(), ValidationRule::MinLength(*value)))
            }
            ("max_length", AttributeValueKind::Int(value)) => {
                Some(("max_length".to_string(), ValidationRule::MaxLength(*value)))
            }
            ("min", AttributeValueKind::Int(value)) => {
                Some(("min".to_string(), ValidationRule::Min(*value)))
            }
            ("max", AttributeValueKind::Int(value)) => {
                Some(("max".to_string(), ValidationRule::Max(*value)))
            }
            ("one_of", AttributeValueKind::String(value)) => {
                Some(("one_of".to_string(), ValidationRule::OneOf(value.clone())))
            }
            ("custom", AttributeValueKind::String(value)) => {
                Some(("custom".to_string(), ValidationRule::Custom(value.clone())))
            }
            _ => None,
        },
    }
}

fn positional_string(attr: &Attribute) -> Option<String> {
    if attr.args.len() != 1 {
        return None;
    }
    match &attr.args[0] {
        AttributeArg::Positional(AttributeValue {
            kind: AttributeValueKind::String(value),
            ..
        }) => Some(value.clone()),
        _ => None,
    }
}

fn named_int(attr: &Attribute, expected: &str) -> Option<i64> {
    if attr.args.len() != 1 {
        return None;
    }
    match &attr.args[0] {
        AttributeArg::Named {
            name,
            value:
                AttributeValue {
                    kind: AttributeValueKind::Int(value),
                    ..
                },
            ..
        } if name == expected => Some(*value),
        _ => None,
    }
}

fn type_repr(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeKind::Unit => "()".to_string(),
        TypeKind::Named { name, args } => {
            if args.is_empty() {
                name.clone()
            } else {
                format!(
                    "{}[{}]",
                    name,
                    args.iter().map(type_repr).collect::<Vec<_>>().join(", ")
                )
            }
        }
        TypeKind::DynTrait { trait_name } => format!("dyn {trait_name}"),
        TypeKind::Hole => "_".to_string(),
    }
}

fn sanitize_ident(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "generated".to_string()
    } else {
        out
    }
}

fn is_identifier_path(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    value.split('.').all(|segment| {
        let mut chars = segment.chars();
        match chars.next() {
            Some(first) if first.is_ascii_alphabetic() || first == '_' => {
                chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            }
            _ => false,
        }
    })
}

fn escape_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn indent_block(value: &str, spaces: usize) -> String {
    let indent = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::generate;
    use crate::parser;

    #[test]
    fn generates_stable_route_filter_and_validator_source() {
        let source = r#"
module app.main;

import aicore_web_core.web;
import aicore_web_validation.validation;

#[filter(order = 10)]
struct TraceFilter {}

struct CreateUser {
    #[validate(required, min_length = 2, max_length = 20, one_of = "Ada|Grace", custom = "validators.check_name")]
    name: String,
    #[validate(min = 0, max = 130)]
    age: Int,
}

#[get("/users/:id")]
async fn get_user(req: RequestContext) -> ResponseEntity {
    web.json_error(200u16, "ok", "ok")
}
"#;
        let (program, diagnostics) = parser::parse(source, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let program = program.expect("program");
        let modules = program
            .items
            .iter()
            .map(|_| Some(vec!["app".to_string(), "main".to_string()]))
            .collect::<Vec<_>>();
        let generated = generate(&program, &modules, "test.aic");
        assert!(
            generated.diagnostics.is_empty(),
            "diagnostics={:#?}",
            generated.diagnostics
        );
        assert_eq!(generated.route_count, 1);
        assert_eq!(generated.filter_count, 1);
        assert_eq!(generated.validator_count, 1);
        assert!(generated
            .source
            .contains("impl Handler[AicoreWebRoute_0_get_user]"));
        assert!(generated
            .source
            .contains("web.add_filter(app0, 10, TraceFilter {})"));
        assert!(generated
            .source
            .contains("web.add_route(app1, \"GET\", \"/users/:id\""));
        assert!(generated.source.contains("impl Validate[CreateUser]"));
        assert!(generated.source.contains("validation.one_of_string_list"));
        assert!(generated.source.contains("validators.check_name"));
    }

    #[test]
    fn duplicate_route_and_filter_metadata_are_diagnostics() {
        let source = r#"
module app.main;

#[filter(order = 1)]
struct A {}

#[filter(order = 1)]
struct B {}

#[get("/same")]
async fn one(req: RequestContext) -> ResponseEntity { web.json_error(200u16, "ok", "ok") }

#[get("/same")]
async fn two(req: RequestContext) -> ResponseEntity { web.json_error(200u16, "ok", "ok") }
"#;
        let (program, diagnostics) = parser::parse(source, "test.aic");
        assert!(diagnostics.is_empty(), "diagnostics={diagnostics:#?}");
        let program = program.expect("program");
        let modules = program
            .items
            .iter()
            .map(|_| Some(vec!["app".to_string(), "main".to_string()]))
            .collect::<Vec<_>>();
        let generated = generate(&program, &modules, "test.aic");
        assert!(generated.source.is_empty());
        assert_eq!(
            generated
                .diagnostics
                .iter()
                .filter(|diag| diag.code == "E1096")
                .count(),
            2
        );
    }
}

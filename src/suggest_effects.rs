use std::collections::BTreeMap;

use serde::Serialize;

use crate::ast::{decode_internal_const, decode_internal_type_alias};
use crate::driver::FrontendOutput;
use crate::ir;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SuggestEffectsResponse {
    pub suggestions: Vec<FunctionEffectSuggestion>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FunctionEffectSuggestion {
    pub function: String,
    pub current_effects: Vec<String>,
    pub required_effects: Vec<String>,
    pub missing_effects: Vec<String>,
    pub reason: BTreeMap<String, String>,
}

pub fn analyze(front: &FrontendOutput) -> SuggestEffectsResponse {
    let mut functions = collect_user_functions(&front.ir);
    functions.sort_by(|left, right| left.name.cmp(&right.name));

    let mut suggestions = Vec::new();
    for function in functions {
        let current = function
            .effects
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let required = front
            .typecheck
            .function_effect_usage
            .get(&function.name)
            .cloned()
            .unwrap_or_else(|| current.clone());
        let missing = required.difference(&current).cloned().collect::<Vec<_>>();
        if missing.is_empty() {
            continue;
        }

        let current_effects = current.into_iter().collect::<Vec<_>>();
        let required_effects = required.into_iter().collect::<Vec<_>>();
        let reasons_for_function = front.typecheck.function_effect_reasons.get(&function.name);
        let reason = required_effects
            .iter()
            .map(|effect| {
                let chain = reasons_for_function
                    .and_then(|by_effect| by_effect.get(effect))
                    .cloned()
                    .unwrap_or_else(|| vec![function.name.clone()]);
                (effect.clone(), chain.join(" -> "))
            })
            .collect::<BTreeMap<_, _>>();

        suggestions.push(FunctionEffectSuggestion {
            function: function.name.clone(),
            current_effects,
            required_effects,
            missing_effects: missing,
            reason,
        });
    }

    SuggestEffectsResponse { suggestions }
}

fn collect_user_functions(program: &ir::Program) -> Vec<&ir::Function> {
    program
        .items
        .iter()
        .filter_map(|item| {
            let ir::Item::Function(func) = item else {
                return None;
            };
            if decode_internal_type_alias(&func.name).is_some()
                || decode_internal_const(&func.name).is_some()
            {
                return None;
            }
            Some(func)
        })
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::driver::{run_frontend_with_options, FrontendOptions};
    use crate::suggest_effects::analyze;

    #[test]
    fn suggest_effects_reports_multi_level_transitive_paths() {
        let project = tempdir().expect("temp project");
        let source_path = project.path().join("main.aic");
        fs::write(
            &source_path,
            concat!(
                "module suggest.effect_inference;\n",
                "import std.io;\n",
                "fn leaf() -> () effects { io } {\n",
                "    print_int(1)\n",
                "}\n",
                "fn middle() -> () {\n",
                "    leaf()\n",
                "}\n",
                "fn top() -> Int {\n",
                "    middle();\n",
                "    0\n",
                "}\n",
            ),
        )
        .expect("write source");

        let front = run_frontend_with_options(&source_path, FrontendOptions { offline: false })
            .expect("frontend");
        let first = analyze(&front);
        let second = analyze(&front);
        assert_eq!(first, second, "suggestions must be deterministic");

        let middle = first
            .suggestions
            .iter()
            .find(|entry| entry.function == "middle")
            .expect("middle suggestion");
        assert_eq!(middle.current_effects, Vec::<String>::new());
        assert_eq!(middle.required_effects, vec!["io".to_string()]);
        assert_eq!(middle.missing_effects, vec!["io".to_string()]);
        assert_eq!(
            middle.reason.get("io").map(String::as_str),
            Some("middle -> leaf")
        );

        let top = first
            .suggestions
            .iter()
            .find(|entry| entry.function == "top")
            .expect("top suggestion");
        assert_eq!(top.current_effects, Vec::<String>::new());
        assert_eq!(top.required_effects, vec!["io".to_string()]);
        assert_eq!(top.missing_effects, vec!["io".to_string()]);
        assert_eq!(
            top.reason.get("io").map(String::as_str),
            Some("top -> middle -> leaf")
        );
    }
}

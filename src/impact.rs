use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt::{self, Display};

use serde::Serialize;

use crate::driver::FrontendOutput;
use crate::ir;

const ROOT_MODULE: &str = "<root>";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImpactReport {
    pub function: String,
    pub direct_callers: Vec<String>,
    pub transitive_callers: Vec<String>,
    pub affected_tests: Vec<String>,
    pub affected_contracts: Vec<String>,
    pub blast_radius: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImpactError {
    UnknownFunction(String),
    AmbiguousFunction {
        function: String,
        modules: Vec<String>,
    },
}

impl Display for ImpactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFunction(function) => write!(f, "unknown function `{function}`"),
            Self::AmbiguousFunction { function, modules } => write!(
                f,
                "function `{function}` is ambiguous across modules: {}",
                modules.join(", ")
            ),
        }
    }
}

impl Error for ImpactError {}

pub fn analyze(
    front: &FrontendOutput,
    requested_function: &str,
) -> Result<ImpactReport, ImpactError> {
    let inverse = build_inverse_callers(&front.typecheck.call_graph);
    let (requested_module, function_name) = parse_function_selector(requested_function.trim());
    if function_name.is_empty() {
        return Err(ImpactError::UnknownFunction(requested_function.to_string()));
    }

    let known_modules = front.resolution.function_modules.get(&function_name);
    if let Some(module) = requested_module.as_deref() {
        let Some(modules) = known_modules else {
            return Err(ImpactError::UnknownFunction(requested_function.to_string()));
        };
        if !modules.contains(module) {
            return Err(ImpactError::UnknownFunction(requested_function.to_string()));
        }
    } else if let Some(modules) = known_modules {
        if modules.len() > 1 {
            return Err(ImpactError::AmbiguousFunction {
                function: function_name.clone(),
                modules: modules.iter().cloned().collect(),
            });
        }
    }

    let known_function = known_modules.is_some()
        || front.typecheck.call_graph.contains_key(&function_name)
        || inverse.contains_key(&function_name);
    if !known_function {
        return Err(ImpactError::UnknownFunction(requested_function.to_string()));
    }

    let direct_callers = inverse.get(&function_name).cloned().unwrap_or_default();
    let transitive_callers = collect_transitive_callers(&inverse, &direct_callers);
    let contract_functions = collect_contract_functions(&front.ir);

    let mut impacted_functions = BTreeSet::new();
    impacted_functions.insert(function_name.clone());
    impacted_functions.extend(direct_callers.iter().cloned());
    impacted_functions.extend(transitive_callers.iter().cloned());

    let affected_tests = impacted_functions
        .iter()
        .filter(|name| is_test_function(name, front.resolution.function_modules.get(*name)))
        .map(|name| display_function(name, &front.resolution.function_modules, None))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let affected_contracts = impacted_functions
        .iter()
        .filter(|name| contract_functions.contains(*name))
        .map(|name| display_function(name, &front.resolution.function_modules, None))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let selected_module = requested_module.or_else(|| {
        known_modules.and_then(|modules| {
            if modules.len() == 1 {
                modules.iter().next().cloned()
            } else {
                None
            }
        })
    });

    let blast_radius = classify_blast_radius(
        direct_callers.len(),
        transitive_callers.len(),
        affected_tests.len(),
        affected_contracts.len(),
    );

    Ok(ImpactReport {
        function: display_function(
            &function_name,
            &front.resolution.function_modules,
            selected_module.as_deref(),
        ),
        direct_callers: direct_callers
            .iter()
            .map(|name| display_function(name, &front.resolution.function_modules, None))
            .collect(),
        transitive_callers: transitive_callers
            .iter()
            .map(|name| display_function(name, &front.resolution.function_modules, None))
            .collect(),
        affected_tests,
        affected_contracts,
        blast_radius: blast_radius.to_string(),
    })
}

fn parse_function_selector(raw: &str) -> (Option<String>, String) {
    if let Some((module, function)) = raw.rsplit_once("::") {
        if !module.is_empty() && !function.is_empty() {
            return (Some(module.to_string()), function.to_string());
        }
    }
    if let Some((module, function)) = raw.rsplit_once('.') {
        if !module.is_empty() && !function.is_empty() {
            return (Some(module.to_string()), function.to_string());
        }
    }
    (None, raw.to_string())
}

fn build_inverse_callers(
    call_graph: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut inverse = BTreeMap::new();
    for (caller, callees) in call_graph {
        inverse.entry(caller.clone()).or_insert_with(BTreeSet::new);
        for callee in callees {
            inverse
                .entry(callee.clone())
                .or_insert_with(BTreeSet::new)
                .insert(caller.clone());
        }
    }
    inverse
}

fn collect_transitive_callers(
    inverse: &BTreeMap<String, BTreeSet<String>>,
    direct_callers: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut queue = VecDeque::new();
    let mut seen = BTreeSet::new();
    let mut transitive = BTreeSet::new();

    for caller in direct_callers {
        queue.push_back(caller.clone());
        seen.insert(caller.clone());
    }

    while let Some(node) = queue.pop_front() {
        let Some(callers) = inverse.get(&node) else {
            continue;
        };
        for caller in callers {
            if seen.insert(caller.clone()) {
                queue.push_back(caller.clone());
                if !direct_callers.contains(caller) {
                    transitive.insert(caller.clone());
                }
            }
        }
    }

    transitive
}

fn collect_contract_functions(program: &ir::Program) -> BTreeSet<String> {
    let mut contract_functions = BTreeSet::new();
    for item in &program.items {
        match item {
            ir::Item::Function(function) => {
                push_contract_function(&mut contract_functions, function);
            }
            ir::Item::Trait(trait_def) => {
                for method in &trait_def.methods {
                    push_contract_function(&mut contract_functions, method);
                }
            }
            ir::Item::Impl(impl_def) => {
                for method in &impl_def.methods {
                    push_contract_function(&mut contract_functions, method);
                }
            }
            ir::Item::Struct(_) | ir::Item::Enum(_) => {}
        }
    }
    contract_functions
}

fn push_contract_function(contract_functions: &mut BTreeSet<String>, function: &ir::Function) {
    if function.requires.is_some() || function.ensures.is_some() {
        contract_functions.insert(function.name.clone());
    }
}

fn is_test_function(function_name: &str, modules: Option<&BTreeSet<String>>) -> bool {
    if function_name.starts_with("test_") || function_name.ends_with("_test") {
        return true;
    }
    modules
        .map(|entries| entries.iter().any(|module| module_is_test_like(module)))
        .unwrap_or(false)
}

fn module_is_test_like(module: &str) -> bool {
    if module == ROOT_MODULE {
        return false;
    }
    module
        .split('.')
        .any(|segment| matches!(segment, "test" | "tests" | "spec" | "specs" | "harness"))
}

fn display_function(
    function_name: &str,
    modules_by_function: &BTreeMap<String, BTreeSet<String>>,
    preferred_module: Option<&str>,
) -> String {
    if let Some(module) = preferred_module {
        if module != ROOT_MODULE {
            return format!("{module}.{function_name}");
        }
        return function_name.to_string();
    }

    if let Some(modules) = modules_by_function.get(function_name) {
        if modules.len() == 1 {
            let module = modules.iter().next().expect("single module");
            if module != ROOT_MODULE {
                return format!("{module}.{function_name}");
            }
        }
    }

    function_name.to_string()
}

fn classify_blast_radius(
    direct_callers_count: usize,
    transitive_callers_count: usize,
    affected_tests_count: usize,
    affected_contracts_count: usize,
) -> &'static str {
    let caller_count = direct_callers_count + transitive_callers_count;
    let untested_impact_zone = caller_count > 0 && affected_tests_count == 0;
    if untested_impact_zone {
        return "large";
    }

    let score = caller_count + affected_contracts_count;
    if score <= 2 {
        "small"
    } else if score <= 6 {
        "medium"
    } else {
        "large"
    }
}

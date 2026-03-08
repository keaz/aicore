use serde::{Deserialize, Serialize};

use crate::diagnostic_explain::explain;
use crate::diagnostics::Diagnostic;

pub const DIAGNOSTIC_REASONING_SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticReasoning {
    pub schema_version: String,
    pub strategy: String,
    pub summary: String,
    pub confidence: u8,
    pub evidence: Vec<String>,
    pub hypotheses: Vec<DiagnosticHypothesis>,
    pub next_actions: Vec<String>,
    pub spec_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticHypothesis {
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub confidence: u8,
}

pub fn derive_reasoning(diag: &Diagnostic) -> Option<DiagnosticReasoning> {
    let reasoning = match diag.code.as_str() {
        "E1033" => reasoning_missing_semicolon(diag),
        "E1100" => reasoning_duplicate_symbol(diag),
        "E1214" => reasoning_type_mismatch(diag),
        "E1218" => reasoning_unknown_callable(diag),
        "E1250" => reasoning_generic_arity(diag),
        "E2001" => reasoning_missing_effects(diag),
        "E2102" => reasoning_visibility_or_intrinsic(diag),
        _ => return None,
    };
    Some(finalize_reasoning(diag, reasoning))
}

fn reasoning_missing_semicolon(diag: &Diagnostic) -> DiagnosticReasoning {
    let mut evidence = base_evidence(diag);
    evidence.push(
        "E1033 is emitted when the parser recovers from a missing `;` after a let binding."
            .to_string(),
    );
    if diag
        .suggested_fixes
        .iter()
        .any(|fix| fix.replacement.as_deref() == Some(";"))
    {
        evidence.push(
            "The compiler produced a safe fix that inserts a semicolon at the binding tail."
                .to_string(),
        );
    }

    DiagnosticReasoning {
        schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
        strategy: "parser-missing-semicolon".to_string(),
        summary: "Parser recovery indicates that a `let` binding was not terminated with `;`.".to_string(),
        confidence: 98,
        evidence,
        hypotheses: vec![
            hypothesis(
                "missing-terminator",
                "Semicolon omitted after the binding expression",
                "The parser advanced past the let-binding expression and could not find the required statement terminator.",
                98,
            ),
            hypothesis(
                "cascade-recovery",
                "Later parser errors may be secondary recovery noise",
                "After the missing `;`, later parse spans can shift until the parser resynchronizes.",
                63,
            ),
        ],
        next_actions: action_list(
            diag,
            [
                "Apply the suggested semicolon fix.".to_string(),
                "Re-run `aic check --json` after the syntax repair to discard recovery noise.".to_string(),
            ],
        ),
        spec_refs: Vec::new(),
    }
}

fn reasoning_duplicate_symbol(diag: &Diagnostic) -> DiagnosticReasoning {
    let symbol = capture_between(&diag.message, "duplicate symbol '", "'")
        .unwrap_or("this symbol")
        .to_string();
    let kinds = capture_between(&diag.message, "kinds '", "' and '")
        .zip(capture_between(&diag.message, "' and '", "'"))
        .map(|(left, right)| format!("{left} vs {right}"))
        .unwrap_or_else(|| "multiple declarations".to_string());

    let mut evidence = base_evidence(diag);
    evidence.push(format!(
        "Resolver observed a duplicate declaration for '{}' within the same module namespace.",
        symbol
    ));
    evidence.push(format!("Conflicting declaration kinds: {kinds}."));

    DiagnosticReasoning {
        schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
        strategy: "resolver-duplicate-symbol".to_string(),
        summary: format!(
            "Name resolution found more than one declaration competing for '{}'.",
            symbol
        ),
        confidence: 97,
        evidence,
        hypotheses: vec![
            hypothesis(
                "same-module-collision",
                "Two declarations use the same exported name",
                "The resolver rejects duplicate value/type names per module because later lookups would become ambiguous.",
                97,
            ),
            hypothesis(
                "generated-helper-collision",
                "A helper alias or synthesized declaration reused the same identifier",
                "Internal type aliases, constants, or generated helpers can still collide with user-written names in the same namespace.",
                68,
            ),
        ],
        next_actions: action_list(
            diag,
            [
                format!("Rename one declaration so '{}' is unique inside the module.", symbol),
                "If the conflict came from a generated helper, pick a user-facing name that cannot overlap with synthesized symbols.".to_string(),
            ],
        ),
        spec_refs: Vec::new(),
    }
}

fn reasoning_type_mismatch(diag: &Diagnostic) -> DiagnosticReasoning {
    if let Some((target, expected, found)) = parse_argument_type_mismatch(&diag.message) {
        return DiagnosticReasoning {
            schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
            strategy: "call-argument-type-mismatch".to_string(),
            summary: format!(
                "The callable '{target}' resolved successfully, but at least one provided argument type is incompatible."
            ),
            confidence: 96,
            evidence: base_evidence(diag),
            hypotheses: vec![
                hypothesis(
                    "wrong-argument-type",
                    "An argument does not match the callable signature",
                    format!(
                        "The resolved signature expects '{expected}', but the supplied argument was typed as '{found}'."
                    ),
                    96,
                ),
                hypothesis(
                    "stale-type-assumption",
                    "The call site was written against a stale or guessed signature",
                    "This often happens when code was drafted before re-checking the actual function signature or inferred type aliases.",
                    71,
                ),
            ],
            next_actions: action_list(
                diag,
                [
                    format!("Change the mismatched argument to type '{expected}', or call a different overload/helper."),
                    format!("Re-check the signature of '{target}' before applying more edits."),
                ],
            ),
            spec_refs: Vec::new(),
        };
    }

    if let Some((target, expected, found)) = parse_arity_mismatch(&diag.message) {
        return DiagnosticReasoning {
            schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
            strategy: "call-arity-mismatch".to_string(),
            summary: format!(
                "The callable '{target}' exists, but the supplied argument count does not match its signature."
            ),
            confidence: 97,
            evidence: base_evidence(diag),
            hypotheses: vec![
                hypothesis(
                    "wrong-arity",
                    "The call site passed the wrong number of arguments",
                    format!(
                        "The resolved callable expects {expected} argument(s), but the current call shape provides {found}."
                    ),
                    97,
                ),
                hypothesis(
                    "guessed-overload",
                    "The caller assumed a different helper variant existed",
                    "AICore does not dispatch by ad-hoc overload sets here; the arity must match the single resolved signature.",
                    69,
                ),
            ],
            next_actions: action_list(
                diag,
                [
                    format!("Adjust the call to pass exactly {expected} argument(s)."),
                    format!("If you intended a different helper than '{target}', qualify or rename the target explicitly."),
                ],
            ),
            spec_refs: Vec::new(),
        };
    }

    if diag.message.contains("not object-safe") {
        return DiagnosticReasoning {
            schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
            strategy: "dyn-object-safety".to_string(),
            summary: "The selected trait cannot be used through `dyn` dispatch under the current object-safety rules.".to_string(),
            confidence: 95,
            evidence: base_evidence(diag),
            hypotheses: vec![
                hypothesis(
                    "trait-shape-incompatible",
                    "The trait shape violates object-safety constraints",
                    "Generic methods, `Self` in non-receiver positions, or `Self`-returning methods prevent safe dyn dispatch.",
                    95,
                ),
                hypothesis(
                    "receiver-contract-mismatch",
                    "A dyn method signature is missing a valid `Self` receiver contract",
                    "Dyn-compatible methods must begin with an explicit receiver that the compiler can erase safely.",
                    74,
                ),
            ],
            next_actions: action_list(
                diag,
                [
                    "Remove the object-safety violation or avoid `dyn` dispatch for this trait.".to_string(),
                    "Prefer concrete generics or non-object-safe traits only in statically dispatched code paths.".to_string(),
                ],
            ),
            spec_refs: Vec::new(),
        };
    }

    fallback_reasoning(
        diag,
        "type-rule-fallback",
        "The compiler resolved the surrounding construct, but a type rule still rejected the current expression shape.",
        83,
        hypothesis(
            "type-system-rejection",
            "A typed expression violated a signature or typing rule",
            "This diagnostic family covers call compatibility, expression typing, and dyn trait object-safety checks.",
            83,
        ),
    )
}

fn reasoning_unknown_callable(diag: &Diagnostic) -> DiagnosticReasoning {
    if let Some(target) = capture_between(&diag.message, "unknown callable '", "'") {
        return DiagnosticReasoning {
            schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
            strategy: "unknown-callable".to_string(),
            summary: format!(
                "The requested callable '{target}' was not found in the current resolution scope."
            ),
            confidence: 96,
            evidence: base_evidence(diag),
            hypotheses: vec![
                hypothesis(
                    "missing-or-mistyped-symbol",
                    "The callable name is misspelled or does not exist",
                    "Resolution could not find any function matching the supplied path after normal module lookup.",
                    96,
                ),
                hypothesis(
                    "qualification-gap",
                    "The callable may exist under a different module path or import alias",
                    "If the symbol exists elsewhere, the current path is incomplete, stale, or not imported into this scope.",
                    72,
                ),
            ],
            next_actions: action_list(
                diag,
                [
                    format!("Verify the exact module-qualified name for '{target}'."),
                    "Run `aic suggest --partial <name> --project .` or inspect `aic context` before drafting follow-up edits.".to_string(),
                ],
            ),
            spec_refs: Vec::new(),
        };
    }

    if diag
        .message
        .contains("unavailable because not all fields have defaults")
    {
        return DiagnosticReasoning {
            schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
            strategy: "default-constructor-unavailable".to_string(),
            summary: "The auto-generated default constructor was rejected because some required fields still lack default values.".to_string(),
            confidence: 94,
            evidence: base_evidence(diag),
            hypotheses: vec![
                hypothesis(
                    "missing-field-defaults",
                    "Not every struct field has a default expression",
                    "AICore only synthesizes `Struct::default` when every field can be initialized without caller input.",
                    94,
                ),
                hypothesis(
                    "premature-constructor-assumption",
                    "The caller assumed an auto-generated helper exists for a partially-specified struct",
                    "Generated helpers only appear when the struct satisfies the compiler's full synthesis preconditions.",
                    68,
                ),
            ],
            next_actions: action_list(
                diag,
                [
                    "Add defaults for every field, or construct the value explicitly.".to_string(),
                    "If generics are involved, provide a concrete type annotation before retrying synthesis.".to_string(),
                ],
            ),
            spec_refs: Vec::new(),
        };
    }

    fallback_reasoning(
        diag,
        "callable-resolution-fallback",
        "Callable resolution failed before typechecking could validate the call shape.",
        82,
        hypothesis(
            "unresolved-call-target",
            "The call target could not be resolved",
            "The compiler did not find a callable symbol for the requested name or synthesized helper.",
            82,
        ),
    )
}

fn reasoning_generic_arity(diag: &Diagnostic) -> DiagnosticReasoning {
    if let Some((name, expected, found)) = parse_generic_arity(&diag.message) {
        return DiagnosticReasoning {
            schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
            strategy: "generic-arity-mismatch".to_string(),
            summary: format!(
                "The type constructor '{name}' was instantiated with the wrong number of generic arguments."
            ),
            confidence: 97,
            evidence: base_evidence(diag),
            hypotheses: vec![
                hypothesis(
                    "wrong-type-argument-count",
                    "The generic arity does not match the declared constructor",
                    format!(
                        "'{name}' expects {expected} generic argument(s), but the current type expression supplies {found}."
                    ),
                    97,
                ),
                hypothesis(
                    "stale-alias-shape",
                    "The call site or type alias was written against an older generic shape",
                    "Arity mismatches often appear after a type alias or std API changed the number of required generic parameters.",
                    66,
                ),
            ],
            next_actions: action_list(
                diag,
                [
                    format!("Update '{name}' to pass exactly {expected} generic argument(s)."),
                    "Re-check any surrounding type aliases or inferred annotations that still assume the old arity.".to_string(),
                ],
            ),
            spec_refs: Vec::new(),
        };
    }

    if diag.message.contains("expected at least 1 type argument") {
        return DiagnosticReasoning {
            schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
            strategy: "fn-type-arity-mismatch".to_string(),
            summary: "The `Fn` type constructor requires at least a return type and was used without enough type arguments.".to_string(),
            confidence: 96,
            evidence: base_evidence(diag),
            hypotheses: vec![
                hypothesis(
                    "missing-fn-type-components",
                    "The function type omitted required generic slots",
                    "A bare `Fn` type is incomplete without at least the return type and any parameter slots.",
                    96,
                ),
                hypothesis(
                    "partial-type-rewrite",
                    "A type rewrite or placeholder was left unfinished",
                    "This commonly happens when function signatures are being refactored and the type constructor was inserted before its arguments.",
                    61,
                ),
            ],
            next_actions: action_list(
                diag,
                [
                    "Use the full `Fn(...) -> Ret` form with the required type arguments.".to_string(),
                    "Finish the partial function-type rewrite before retrying the check.".to_string(),
                ],
            ),
            spec_refs: Vec::new(),
        };
    }

    fallback_reasoning(
        diag,
        "generic-arity-fallback",
        "A generic type or trait application used the wrong number of type parameters.",
        84,
        hypothesis(
            "generic-application-rejected",
            "The compiler rejected a generic application shape",
            "The supplied generic arguments did not match the declared arity of the referenced type constructor.",
            84,
        ),
    )
}

fn reasoning_missing_effects(diag: &Diagnostic) -> DiagnosticReasoning {
    let missing_effects = capture_after_last(&diag.message, ": ")
        .map(split_csv)
        .unwrap_or_default();
    let rendered_effects = if missing_effects.is_empty() {
        "the required effect set".to_string()
    } else {
        missing_effects.join(", ")
    };

    let summary = if let Some(function_name) =
        capture_between(&diag.message, "function '", "' uses undeclared effects")
    {
        format!(
            "The function '{function_name}' performs effectful work but its signature does not declare {rendered_effects}."
        )
    } else if let Some(target) =
        capture_between(&diag.message, "calling '", "' requires undeclared effects")
    {
        format!(
            "The call to '{target}' is effectful, but the enclosing function has not declared {rendered_effects}."
        )
    } else {
        format!(
            "An effectful operation was reached without declaring {rendered_effects} on the current function."
        )
    };

    let mut evidence = base_evidence(diag);
    evidence.push(format!(
        "Missing effect set inferred from the diagnostic payload: {rendered_effects}."
    ));
    if !diag.suggested_fixes.is_empty() {
        evidence.push("The compiler also emitted a deterministic edit for the required `effects { ... }` clause.".to_string());
    }

    DiagnosticReasoning {
        schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
        strategy: "missing-effects".to_string(),
        summary,
        confidence: 99,
        evidence,
        hypotheses: vec![
            hypothesis(
                "caller-missing-effects-clause",
                "The enclosing function omits required effects",
                format!(
                    "The effect checker tracked effectful behavior and determined that `{rendered_effects}` is used without being declared on the caller."
                ),
                99,
            ),
            hypothesis(
                "pure-wrapper-drift",
                "A previously-pure wrapper started calling effectful helpers",
                "This often appears after introducing IO/time/rand/net/fs helpers without updating the wrapper signature.",
                73,
            ),
        ],
        next_actions: action_list(
            diag,
            [
                format!("Add or update `effects {{ {rendered_effects} }}` on the enclosing function."),
                "If the function should remain pure, move the effectful call into a dedicated helper with an explicit effects clause.".to_string(),
            ],
        ),
        spec_refs: Vec::new(),
    }
}

fn reasoning_visibility_or_intrinsic(diag: &Diagnostic) -> DiagnosticReasoning {
    if let Some(symbol) = capture_between(
        &diag.message,
        "intrinsic symbol '",
        "' is private runtime implementation detail",
    ) {
        return DiagnosticReasoning {
            schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
            strategy: "intrinsic-visibility-violation".to_string(),
            summary: format!(
                "User code attempted to call the private runtime intrinsic '{symbol}' directly."
            ),
            confidence: 98,
            evidence: base_evidence(diag),
            hypotheses: vec![
                hypothesis(
                    "private-intrinsic-access",
                    "A compiler-only intrinsic leaked into user code",
                    "Runtime intrinsics are reserved for lowered codegen paths and are intentionally blocked from direct source-level use.",
                    98,
                ),
                hypothesis(
                    "public-api-bypass",
                    "The call site bypassed the supported std facade",
                    "There is usually a public std API that wraps this intrinsic with the correct type/effect/capability contract.",
                    76,
                ),
            ],
            next_actions: action_list(
                diag,
                [
                    "Replace the intrinsic call with the corresponding public std API.".to_string(),
                    "If no public API exists, add one rather than exposing the runtime symbol directly.".to_string(),
                ],
            ),
            spec_refs: Vec::new(),
        };
    }

    if let Some(symbol) = capture_between(
        &diag.message,
        "symbol '",
        "' is private and not accessible from this module",
    ) {
        return DiagnosticReasoning {
            schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
            strategy: "module-visibility-violation".to_string(),
            summary: format!(
                "The reference to '{symbol}' crossed a visibility boundary and is not accessible from the current module."
            ),
            confidence: 97,
            evidence: base_evidence(diag),
            hypotheses: vec![
                hypothesis(
                    "private-symbol-access",
                    "The target declaration is not exported",
                    "Resolution found the symbol, but its visibility rules prevent use from the current module.",
                    97,
                ),
                hypothesis(
                    "wrong-module-boundary",
                    "The code assumes an internal helper is part of the public module API",
                    "Private module helpers are reachable only from the defining module unless they are promoted to `pub`.",
                    72,
                ),
            ],
            next_actions: action_list(
                diag,
                [
                    format!("Export '{symbol}' with the appropriate `pub` visibility, or stop calling it across modules."),
                    "If the helper should remain private, add a separate public entrypoint instead.".to_string(),
                ],
            ),
            spec_refs: Vec::new(),
        };
    }

    fallback_reasoning(
        diag,
        "visibility-fallback",
        "A symbol was resolved, but module or runtime visibility rules still block the access.",
        85,
        hypothesis(
            "visibility-gate",
            "Visibility rules rejected the symbol reference",
            "This diagnostic family covers module privacy barriers and blocked runtime implementation details.",
            85,
        ),
    )
}

fn fallback_reasoning(
    diag: &Diagnostic,
    strategy: &str,
    summary: &str,
    confidence: u8,
    primary: DiagnosticHypothesis,
) -> DiagnosticReasoning {
    DiagnosticReasoning {
        schema_version: DIAGNOSTIC_REASONING_SCHEMA_VERSION.to_string(),
        strategy: strategy.to_string(),
        summary: summary.to_string(),
        confidence,
        evidence: base_evidence(diag),
        hypotheses: vec![primary],
        next_actions: action_list(diag, []),
        spec_refs: Vec::new(),
    }
}

fn finalize_reasoning(
    diag: &Diagnostic,
    mut reasoning: DiagnosticReasoning,
) -> DiagnosticReasoning {
    reasoning.hypotheses.sort_by(|lhs, rhs| {
        rhs.confidence
            .cmp(&lhs.confidence)
            .then(lhs.kind.cmp(&rhs.kind))
            .then(lhs.title.cmp(&rhs.title))
            .then(lhs.detail.cmp(&rhs.detail))
    });
    dedup_hypotheses(&mut reasoning.hypotheses);
    dedup_preserve_order(&mut reasoning.evidence);
    dedup_preserve_order(&mut reasoning.next_actions);

    let explanation = explain(&diag.code);
    let mut spec_refs = explanation.spec_refs;
    spec_refs.extend(reasoning.spec_refs);
    spec_refs.sort();
    spec_refs.dedup();
    reasoning.spec_refs = spec_refs;
    reasoning
}

fn base_evidence(diag: &Diagnostic) -> Vec<String> {
    let mut evidence = vec![format!("Compiler message: {}", diag.message)];
    if !diag.help.is_empty() {
        evidence.push(format!("Compiler help: {}", diag.help.join(" | ")));
    }
    if !diag.suggested_fixes.is_empty() {
        evidence.push(format!(
            "Machine-applicable fixes attached: {}",
            diag.suggested_fixes.len()
        ));
    }
    evidence
}

fn action_list<const N: usize>(diag: &Diagnostic, extras: [String; N]) -> Vec<String> {
    let mut actions = extras.into_iter().collect::<Vec<_>>();
    actions.extend(diag.help.iter().cloned());
    actions.extend(explain(&diag.code).remediation);
    dedup_preserve_order(&mut actions);
    actions
}

fn hypothesis(
    kind: &str,
    title: &str,
    detail: impl Into<String>,
    confidence: u8,
) -> DiagnosticHypothesis {
    DiagnosticHypothesis {
        kind: kind.to_string(),
        title: title.to_string(),
        detail: detail.into(),
        confidence,
    }
}

fn dedup_preserve_order(values: &mut Vec<String>) {
    let mut seen = std::collections::BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn dedup_hypotheses(values: &mut Vec<DiagnosticHypothesis>) {
    let mut seen = std::collections::BTreeSet::new();
    values.retain(|value| {
        seen.insert((
            value.kind.clone(),
            value.title.clone(),
            value.detail.clone(),
            value.confidence,
        ))
    });
}

fn parse_argument_type_mismatch(message: &str) -> Option<(String, String, String)> {
    let target = capture_between(message, " to '", "' expected '")?;
    let expected = capture_between(message, " expected '", "', found '")?;
    let found = capture_between(message, "', found '", "'")?;
    Some((target.to_string(), expected.to_string(), found.to_string()))
}

fn parse_arity_mismatch(message: &str) -> Option<(String, String, String)> {
    let target = capture_between(message, "call to '", "' expected ")?;
    let expected = capture_between(message, "' expected ", " argument(s), found ")?;
    let found = capture_after_last(message, "found ")?;
    Some((target.to_string(), expected.to_string(), found.to_string()))
}

fn parse_generic_arity(message: &str) -> Option<(String, String, String)> {
    let name = capture_between(message, "generic arity mismatch for '", "': expected ")?;
    let expected = capture_between(message, "': expected ", ", found ")?;
    let found = capture_after_last(message, "found ")?;
    Some((name.to_string(), expected.to_string(), found.to_string()))
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn capture_between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = text.find(start)? + start.len();
    let rest = text.get(from..)?;
    let until = rest.find(end)?;
    rest.get(..until)
}

fn capture_after_last<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let from = text.rfind(marker)? + marker.len();
    text.get(from..)
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::Diagnostic;
    use crate::span::Span;

    use super::derive_reasoning;

    fn diag(code: &str, message: &str) -> Diagnostic {
        Diagnostic::error(code, message, "main.aic", Span::new(10, 20))
    }

    #[test]
    fn selected_high_frequency_codes_emit_reasoning() {
        let cases = [
            diag("E1033", "expected ';' after let binding"),
            diag("E1100", "duplicate symbol 'answer', kinds 'fn' and 'const'"),
            diag(
                "E1214",
                "argument 1 to 'math.add' expected 'Int', found 'String'",
            ),
            diag("E1218", "unknown callable 'math.adz'"),
            diag(
                "E1250",
                "generic arity mismatch for 'Result': expected 2, found 1",
            ),
            diag(
                "E2001",
                "calling 'io_fn' requires undeclared effects: io, fs",
            ),
            diag(
                "E2102",
                "symbol 'app.secret.helper' is private and not accessible from this module",
            ),
        ];

        for diagnostic in cases {
            let reasoning = derive_reasoning(&diagnostic).expect("reasoning");
            assert_eq!(reasoning.schema_version, "1.0");
            assert!((1..=100).contains(&reasoning.confidence));
            assert!(!reasoning.hypotheses.is_empty());
        }
    }

    #[test]
    fn hypotheses_are_sorted_by_confidence_then_identity() {
        let diagnostic = diag("E1033", "expected ';' after let binding");
        let reasoning = derive_reasoning(&diagnostic).expect("reasoning");
        let confidences = reasoning
            .hypotheses
            .iter()
            .map(|hypothesis| hypothesis.confidence)
            .collect::<Vec<_>>();
        assert_eq!(confidences, vec![98, 63]);
    }

    #[test]
    fn unsupported_diagnostics_do_not_emit_reasoning() {
        let diagnostic = diag("E4005", "contract discharged");
        assert!(derive_reasoning(&diagnostic).is_none());
    }
}

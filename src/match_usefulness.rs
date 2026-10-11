//! Pattern-match exhaustiveness and reachability.
//!
//! Implements the usefulness algorithm from Luc Maranget, "Warnings for
//! pattern matching" (JFP 2007). Patterns are deconstructed into
//! constructors applied to sub-patterns, so nested payloads, tuples, structs,
//! literals, wildcards and or-patterns are all handled uniformly:
//!
//! - an arm is unreachable when its pattern is not useful with respect to the
//!   (unguarded) arms above it;
//! - a match is exhaustive when a wildcard is not useful with respect to all
//!   unguarded arms; otherwise the algorithm produces witness patterns of
//!   values that no arm matches.
//!
//! Types are described by a [`PatternTypes`] oracle so the algorithm itself
//! is independent of the checker. Types with infinitely many values (`Int`,
//! `String`, `Char`, generic parameters, unknown types) have no complete
//! constructor set; only a wildcard or binding covers them.

use std::collections::BTreeSet;

/// A pattern constructor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ctor {
    /// Enum, `Option` or `Result` variant.
    Variant(String),
    Bool(bool),
    Unit,
    /// Tuple of the given arity.
    Tuple(usize),
    /// Struct with the given name; fields follow [`PatternTypes::field_types`].
    Struct(String),
    /// Integer, string or char literal, rendered as written in source.
    Literal(String),
}

/// A deconstructed pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pat {
    Wild,
    Ctor(Ctor, Vec<Pat>),
    Or(Vec<Pat>),
}

/// Type information needed by the algorithm.
pub trait PatternTypes {
    /// Every constructor of `ty` when the type has finitely many, `None`
    /// otherwise (integers, strings, chars, generic and unknown types).
    fn constructors(&self, ty: &str) -> Option<Vec<Ctor>>;

    /// Types of the sub-patterns of `ctor` for a value of type `ty`.
    fn field_types(&self, ty: &str, ctor: &Ctor) -> Vec<String>;

    /// Field names of a struct constructor, in the order of `field_types`.
    fn struct_field_names(&self, ty: &str) -> Vec<String>;
}

/// Maximum number of witnesses reported for one match.
pub const MAX_WITNESSES: usize = 8;

/// Whether `row` matches some value that no row of `matrix` matches.
pub fn is_useful(
    matrix: &[Vec<Pat>],
    row: &[Pat],
    tys: &[String],
    types: &dyn PatternTypes,
) -> bool {
    let Some((head, rest)) = row.split_first() else {
        return matrix.is_empty();
    };
    let ty = &tys[0];
    match head {
        Pat::Ctor(ctor, args) => {
            let fields = types.field_types(ty, ctor);
            let specialized = specialize(matrix, ctor, fields.len());
            let mut next_row = pad(args, fields.len());
            next_row.extend(rest.iter().cloned());
            let next_tys = concat(&fields, &tys[1..]);
            is_useful(&specialized, &next_row, &next_tys, types)
        }
        Pat::Or(alternatives) => alternatives.iter().any(|alternative| {
            let mut next_row = vec![alternative.clone()];
            next_row.extend(rest.iter().cloned());
            is_useful(matrix, &next_row, tys, types)
        }),
        Pat::Wild => {
            let seen = head_constructors(matrix);
            match complete_signature(ty, &seen, types) {
                Some(all) => all.iter().any(|ctor| {
                    let fields = types.field_types(ty, ctor);
                    let specialized = specialize(matrix, ctor, fields.len());
                    let mut next_row = vec![Pat::Wild; fields.len()];
                    next_row.extend(rest.iter().cloned());
                    let next_tys = concat(&fields, &tys[1..]);
                    is_useful(&specialized, &next_row, &next_tys, types)
                }),
                None => is_useful(&default_matrix(matrix), rest, &tys[1..], types),
            }
        }
    }
}

/// Rows of patterns (one per column type in `tys`) that no row of `matrix`
/// matches. Empty when the matrix is exhaustive. At most `limit` rows.
pub fn missing_rows(
    matrix: &[Vec<Pat>],
    tys: &[String],
    types: &dyn PatternTypes,
    limit: usize,
) -> Vec<Vec<Pat>> {
    if limit == 0 {
        return Vec::new();
    }
    let Some(ty) = tys.first() else {
        return if matrix.is_empty() {
            vec![Vec::new()]
        } else {
            Vec::new()
        };
    };
    // For a finite type that the column mentions, split on every
    // constructor: unmentioned constructors specialize to the wildcard rows,
    // so this enumerates every uncovered combination. Otherwise (an infinite
    // type, or a column of wildcards only) any head value works: `_`.
    let seen = head_constructors(matrix);
    if let Some(all) = types.constructors(ty).filter(|_| !seen.is_empty()) {
        let mut out = Vec::new();
        for ctor in all {
            let fields = types.field_types(ty, &ctor);
            let specialized = specialize(matrix, &ctor, fields.len());
            let next_tys = concat(&fields, &tys[1..]);
            for row in missing_rows(&specialized, &next_tys, types, limit - out.len()) {
                let (args, rest) = row.split_at(fields.len());
                let mut witness = vec![Pat::Ctor(ctor.clone(), args.to_vec())];
                witness.extend(rest.iter().cloned());
                out.push(witness);
                if out.len() >= limit {
                    return out;
                }
            }
        }
        return out;
    }
    missing_rows(&default_matrix(matrix), &tys[1..], types, limit)
        .into_iter()
        .map(|rest| {
            let mut witness = vec![Pat::Wild];
            witness.extend(rest);
            witness
        })
        .collect()
}

/// Renders a witness pattern for diagnostics and fixes.
pub fn render(pat: &Pat, ty: &str, types: &dyn PatternTypes) -> String {
    match pat {
        Pat::Wild => "_".to_string(),
        Pat::Or(alternatives) => alternatives
            .iter()
            .map(|alternative| render(alternative, ty, types))
            .collect::<Vec<_>>()
            .join(" | "),
        Pat::Ctor(ctor, args) => {
            let fields = types.field_types(ty, ctor);
            let rendered_args = args
                .iter()
                .zip(fields.iter().chain(std::iter::repeat(&"<?>".to_string())))
                .map(|(arg, field_ty)| render(arg, field_ty, types))
                .collect::<Vec<_>>();
            match ctor {
                Ctor::Variant(name) if rendered_args.is_empty() => name.clone(),
                Ctor::Variant(name) => format!("{}({})", name, rendered_args.join(", ")),
                Ctor::Bool(value) => value.to_string(),
                Ctor::Unit => "()".to_string(),
                Ctor::Tuple(_) => format!("({})", rendered_args.join(", ")),
                Ctor::Struct(name) => {
                    let names = types.struct_field_names(ty);
                    let shown = names
                        .iter()
                        .zip(rendered_args.iter())
                        .filter(|(_, value)| value.as_str() != "_")
                        .map(|(field, value)| format!("{field}: {value}"))
                        .collect::<Vec<_>>();
                    if shown.is_empty() {
                        format!("{name} {{ .. }}")
                    } else if shown.len() == names.len() {
                        format!("{name} {{ {} }}", shown.join(", "))
                    } else {
                        format!("{name} {{ {}, .. }}", shown.join(", "))
                    }
                }
                Ctor::Literal(text) => text.clone(),
            }
        }
    }
}

/// The full constructor list of `ty` if every constructor appears in
/// `seen`; `None` if some constructor is missing or the type is infinite.
fn complete_signature(
    ty: &str,
    seen: &BTreeSet<Ctor>,
    types: &dyn PatternTypes,
) -> Option<Vec<Ctor>> {
    let all = types.constructors(ty)?;
    if all.iter().all(|ctor| seen.contains(ctor)) {
        Some(all)
    } else {
        None
    }
}

fn head_constructors(matrix: &[Vec<Pat>]) -> BTreeSet<Ctor> {
    let mut out = BTreeSet::new();
    for row in matrix {
        if let Some(head) = row.first() {
            collect_constructors(head, &mut out);
        }
    }
    out
}

fn collect_constructors(pat: &Pat, out: &mut BTreeSet<Ctor>) {
    match pat {
        Pat::Wild => {}
        Pat::Ctor(ctor, _) => {
            out.insert(ctor.clone());
        }
        Pat::Or(alternatives) => {
            for alternative in alternatives {
                collect_constructors(alternative, out);
            }
        }
    }
}

/// Rows whose head matches `ctor`, with the head replaced by its `arity`
/// sub-patterns.
fn specialize(matrix: &[Vec<Pat>], ctor: &Ctor, arity: usize) -> Vec<Vec<Pat>> {
    let mut out = Vec::new();
    for row in matrix {
        specialize_row(row, ctor, arity, &mut out);
    }
    out
}

fn specialize_row(row: &[Pat], ctor: &Ctor, arity: usize, out: &mut Vec<Vec<Pat>>) {
    let Some((head, rest)) = row.split_first() else {
        return;
    };
    match head {
        Pat::Wild => {
            let mut next = vec![Pat::Wild; arity];
            next.extend(rest.iter().cloned());
            out.push(next);
        }
        Pat::Ctor(head_ctor, args) if head_ctor == ctor => {
            let mut next = pad(args, arity);
            next.extend(rest.iter().cloned());
            out.push(next);
        }
        Pat::Ctor(..) => {}
        Pat::Or(alternatives) => {
            for alternative in alternatives {
                let mut expanded = vec![alternative.clone()];
                expanded.extend(rest.iter().cloned());
                specialize_row(&expanded, ctor, arity, out);
            }
        }
    }
}

/// Rows whose head is a wildcard, with the head removed.
fn default_matrix(matrix: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
    let mut out = Vec::new();
    for row in matrix {
        default_row(row, &mut out);
    }
    out
}

fn default_row(row: &[Pat], out: &mut Vec<Vec<Pat>>) {
    let Some((head, rest)) = row.split_first() else {
        return;
    };
    match head {
        Pat::Wild => out.push(rest.to_vec()),
        Pat::Ctor(..) => {}
        Pat::Or(alternatives) => {
            for alternative in alternatives {
                let mut expanded = vec![alternative.clone()];
                expanded.extend(rest.iter().cloned());
                default_row(&expanded, out);
            }
        }
    }
}

/// `args` padded with wildcards (or truncated) to exactly `arity` patterns,
/// so malformed patterns that the checker already reported cannot desync
/// the matrix columns.
fn pad(args: &[Pat], arity: usize) -> Vec<Pat> {
    let mut out = args.iter().take(arity).cloned().collect::<Vec<_>>();
    out.resize(arity, Pat::Wild);
    out
}

fn concat(front: &[String], back: &[String]) -> Vec<String> {
    let mut out = front.to_vec();
    out.extend(back.iter().cloned());
    out
}

#[cfg(test)]
mod tests {
    use super::{is_useful, missing_rows, render, Ctor, Pat, PatternTypes, MAX_WITNESSES};

    /// Small type universe: `Color` = Red | Green | Blue, `Option[T]`,
    /// `Bool`, tuples `(A, B)` written `Tuple[A, B]`, struct `Pair` with
    /// fields `a: Bool, b: Color`, and the infinite `Int`.
    struct TestTypes;

    fn args_of(ty: &str) -> Vec<String> {
        let Some(open) = ty.find('[') else {
            return Vec::new();
        };
        let inner = &ty[open + 1..ty.len() - 1];
        let mut out = Vec::new();
        let mut depth = 0;
        let mut start = 0;
        for (idx, ch) in inner.char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(inner[start..idx].trim().to_string());
                    start = idx + 1;
                }
                _ => {}
            }
        }
        out.push(inner[start..].trim().to_string());
        out
    }

    impl PatternTypes for TestTypes {
        fn constructors(&self, ty: &str) -> Option<Vec<Ctor>> {
            let base = ty.split('[').next().unwrap_or(ty);
            Some(match base {
                "Color" => ["Blue", "Green", "Red"]
                    .iter()
                    .map(|name| Ctor::Variant(name.to_string()))
                    .collect(),
                "Option" => vec![
                    Ctor::Variant("None".to_string()),
                    Ctor::Variant("Some".to_string()),
                ],
                "Bool" => vec![Ctor::Bool(false), Ctor::Bool(true)],
                "Tuple" => vec![Ctor::Tuple(args_of(ty).len())],
                "Pair" => vec![Ctor::Struct("Pair".to_string())],
                _ => return None,
            })
        }

        fn field_types(&self, ty: &str, ctor: &Ctor) -> Vec<String> {
            match ctor {
                Ctor::Variant(name) if name == "Some" => args_of(ty),
                Ctor::Tuple(_) => args_of(ty),
                Ctor::Struct(_) => vec!["Bool".to_string(), "Color".to_string()],
                _ => Vec::new(),
            }
        }

        fn struct_field_names(&self, _ty: &str) -> Vec<String> {
            vec!["a".to_string(), "b".to_string()]
        }
    }

    fn v(name: &str) -> Pat {
        Pat::Ctor(Ctor::Variant(name.to_string()), Vec::new())
    }

    fn some(inner: Pat) -> Pat {
        Pat::Ctor(Ctor::Variant("Some".to_string()), vec![inner])
    }

    fn b(value: bool) -> Pat {
        Pat::Ctor(Ctor::Bool(value), Vec::new())
    }

    fn tuple(items: Vec<Pat>) -> Pat {
        Pat::Ctor(Ctor::Tuple(items.len()), items)
    }

    fn int(value: i64) -> Pat {
        Pat::Ctor(Ctor::Literal(value.to_string()), Vec::new())
    }

    fn analyze(ty: &str, arms: &[Pat]) -> (Vec<usize>, Vec<String>) {
        let tys = vec![ty.to_string()];
        let mut matrix: Vec<Vec<Pat>> = Vec::new();
        let mut unreachable = Vec::new();
        for (idx, arm) in arms.iter().enumerate() {
            if !is_useful(&matrix, &[arm.clone()], &tys, &TestTypes) {
                unreachable.push(idx);
            }
            matrix.push(vec![arm.clone()]);
        }
        let missing = missing_rows(&matrix, &tys, &TestTypes, MAX_WITNESSES)
            .iter()
            .map(|row| render(&row[0], ty, &TestTypes))
            .collect();
        (unreachable, missing)
    }

    #[test]
    fn nested_variants_are_distinct_and_missing_payload_is_reported() {
        let (unreachable, missing) = analyze(
            "Option[Color]",
            &[some(v("Red")), some(v("Green")), v("None")],
        );
        assert!(unreachable.is_empty());
        assert_eq!(missing, vec!["Some(Blue)"]);
    }

    #[test]
    fn literal_then_wildcard_payload_is_reachable() {
        let (unreachable, missing) =
            analyze("Option[Int]", &[some(int(1)), some(Pat::Wild), v("None")]);
        assert!(unreachable.is_empty());
        assert!(missing.is_empty());
    }

    #[test]
    fn tuple_missing_combination_is_reported() {
        let (unreachable, missing) = analyze(
            "Tuple[Color, Bool]",
            &[
                tuple(vec![v("Red"), b(true)]),
                tuple(vec![v("Green"), Pat::Wild]),
                tuple(vec![v("Blue"), Pat::Wild]),
            ],
        );
        assert!(unreachable.is_empty());
        assert_eq!(missing, vec!["(Red, false)"]);
    }

    #[test]
    fn infinite_domains_need_a_wildcard() {
        let (_, missing) = analyze("Int", &[int(1), int(2)]);
        assert_eq!(missing, vec!["_"]);
        let (unreachable, missing) = analyze("Int", &[int(1), Pat::Wild, int(2)]);
        assert_eq!(unreachable, vec![2]);
        assert!(missing.is_empty());
    }

    #[test]
    fn redundant_arms_after_full_coverage_are_unreachable() {
        let (unreachable, missing) = analyze(
            "Option[Color]",
            &[some(Pat::Wild), v("None"), some(v("Red")), Pat::Wild],
        );
        assert_eq!(unreachable, vec![2, 3]);
        assert!(missing.is_empty());
    }

    #[test]
    fn or_patterns_cover_their_alternatives() {
        let (unreachable, missing) = analyze(
            "Color",
            &[Pat::Or(vec![v("Red"), v("Green")]), v("Blue"), v("Red")],
        );
        assert_eq!(unreachable, vec![2]);
        assert!(missing.is_empty());
    }

    #[test]
    fn struct_fields_are_checked_column_by_column() {
        let pair = |a: Pat, b: Pat| Pat::Ctor(Ctor::Struct("Pair".to_string()), vec![a, b]);
        let (_, missing) = analyze(
            "Pair",
            &[
                pair(super::Pat::Ctor(Ctor::Bool(true), Vec::new()), Pat::Wild),
                pair(Pat::Wild, v("Red")),
            ],
        );
        assert_eq!(
            missing,
            vec!["Pair { a: false, b: Blue }", "Pair { a: false, b: Green }"]
        );
    }

    #[test]
    fn empty_match_reports_every_constructor() {
        let (_, missing) = analyze("Color", &[]);
        assert_eq!(missing, vec!["_"]);
        let (_, missing) = analyze("Color", &[v("Red")]);
        assert_eq!(missing, vec!["Blue", "Green"]);
    }

    /// Brute-force reference: enumerate every value of the small types and
    /// check matching directly.
    #[derive(Debug, Clone, PartialEq)]
    enum Value {
        Ctor(Ctor, Vec<Value>),
    }

    fn values(ty: &str, depth: usize) -> Vec<Value> {
        let types = TestTypes;
        let Some(ctors) = types.constructors(ty) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for ctor in ctors {
            let fields = types.field_types(ty, &ctor);
            let mut combos: Vec<Vec<Value>> = vec![Vec::new()];
            for field in &fields {
                let options = if depth == 0 {
                    Vec::new()
                } else {
                    values(field, depth - 1)
                };
                let mut next = Vec::new();
                for combo in &combos {
                    for option in &options {
                        let mut extended = combo.clone();
                        extended.push(option.clone());
                        next.push(extended);
                    }
                }
                combos = next;
            }
            for combo in combos {
                out.push(Value::Ctor(ctor.clone(), combo));
            }
        }
        out
    }

    fn matches(pat: &Pat, value: &Value) -> bool {
        match pat {
            Pat::Wild => true,
            Pat::Or(alternatives) => alternatives.iter().any(|p| matches(p, value)),
            Pat::Ctor(ctor, args) => {
                let Value::Ctor(value_ctor, fields) = value;
                ctor == value_ctor && args.iter().zip(fields.iter()).all(|(p, f)| matches(p, f))
            }
        }
    }

    fn random_pattern(ty: &str, rng: &mut u64, depth: usize) -> Pat {
        *rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let roll = (*rng >> 33) % 10;
        let types = TestTypes;
        if roll < 3 || depth == 0 {
            return Pat::Wild;
        }
        let ctors = types.constructors(ty).expect("finite type");
        if roll == 9 && ctors.len() > 1 {
            let a = random_pattern(ty, rng, depth - 1);
            let b = random_pattern(ty, rng, depth - 1);
            return Pat::Or(vec![a, b]);
        }
        let ctor = ctors[((*rng >> 40) as usize) % ctors.len()].clone();
        let fields = types.field_types(ty, &ctor);
        let args = fields
            .iter()
            .map(|field| random_pattern(field, rng, depth - 1))
            .collect();
        Pat::Ctor(ctor, args)
    }

    #[test]
    fn agrees_with_brute_force_on_random_matches() {
        let mut rng = 0x5EED_u64;
        for ty in [
            "Option[Color]",
            "Tuple[Color, Bool]",
            "Option[Tuple[Bool, Color]]",
            "Pair",
        ] {
            let universe = values(ty, 3);
            assert!(!universe.is_empty());
            for _ in 0..300 {
                let arm_count = 1 + (rng % 5) as usize;
                let arms = (0..arm_count)
                    .map(|_| random_pattern(ty, &mut rng, 3))
                    .collect::<Vec<_>>();
                let tys = vec![ty.to_string()];
                let mut matrix: Vec<Vec<Pat>> = Vec::new();
                for (idx, arm) in arms.iter().enumerate() {
                    let useful = is_useful(&matrix, &[arm.clone()], &tys, &TestTypes);
                    let brute_useful = universe.iter().any(|value| {
                        matches(arm, value) && !arms[..idx].iter().any(|p| matches(p, value))
                    });
                    assert_eq!(useful, brute_useful, "{ty}: arms {arms:?} arm {idx}");
                    matrix.push(vec![arm.clone()]);
                }
                let missing = missing_rows(&matrix, &tys, &TestTypes, usize::MAX);
                let uncovered = universe
                    .iter()
                    .filter(|value| !arms.iter().any(|p| matches(p, value)))
                    .collect::<Vec<_>>();
                assert_eq!(missing.is_empty(), uncovered.is_empty(), "{ty}: {arms:?}");
                for row in &missing {
                    // Every witness describes only uncovered values.
                    for value in &universe {
                        if matches(&row[0], value) {
                            assert!(
                                !arms.iter().any(|p| matches(p, value)),
                                "{ty}: witness {:?} matches a covered value",
                                row[0]
                            );
                        }
                    }
                }
                for value in &uncovered {
                    assert!(
                        missing.iter().any(|row| matches(&row[0], value)),
                        "{ty}: uncovered value {value:?} has no witness"
                    );
                }
            }
        }
    }
}

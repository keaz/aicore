//! Injective symbol mangling for generic instantiations and other
//! compiler-generated names.
//!
//! The checker records instantiation names in the IR and code generation
//! derives the same names for on-demand instances, so both use this module.
//!
//! Scheme: each component is escaped so that `_` only ever starts a two-or-
//! more character escape (`__` is a literal underscore, `_L`/`_R`/`_C` are
//! `[`, `]` and `,`, `_xHH…_` is any other character as hex code point), and
//! components are joined with `_S`. Spaces are dropped (type strings are
//! compared without them). Because every escape is prefix-free, decoding is
//! unambiguous, so different inputs always give different names:
//! `f[B, Int]` -> `fn_f_SB_SInt` and `f_B[Int]` -> `fn_f__B_SInt`.

/// Name of the instantiation of `name` with `type_args`. `kind_tag` is one
/// of `fn`, `struct`, `enum`. `symbol` disambiguates functions that share a
/// bare name across modules.
pub fn instantiation(
    kind_tag: &str,
    name: &str,
    symbol: Option<u32>,
    type_args: &[String],
) -> String {
    let mut out = String::from(kind_tag);
    out.push('_');
    out.push_str(&component(name));
    if let Some(symbol) = symbol {
        // `#` cannot appear in an identifier, so this never equals a
        // user-written name.
        out.push_str(&component(&format!("#{symbol}")));
    }
    for arg in type_args {
        out.push_str("_S");
        out.push_str(&component(arg));
    }
    out
}

/// `prefix` followed by the escaped `parts`, each introduced by `_S`.
pub fn join(prefix: &str, parts: &[&str]) -> String {
    let mut out = String::from(prefix);
    for part in parts {
        out.push_str("_S");
        out.push_str(&component(part));
    }
    out
}

/// Escapes one component; the result contains only `[A-Za-z0-9_]`.
pub fn component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' => out.push(ch),
            '_' => out.push_str("__"),
            '[' => out.push_str("_L"),
            ']' => out.push_str("_R"),
            ',' => out.push_str("_C"),
            ' ' => {}
            other => out.push_str(&format!("_x{:X}_", other as u32)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn underscore_placement_does_not_collide() {
        let a = instantiation("fn", "f", None, &args(&["B", "Int"]));
        let b = instantiation("fn", "f_B", None, &args(&["Int"]));
        assert_ne!(a, b);
        assert_eq!(a, "fn_f_SB_SInt");
        assert_eq!(b, "fn_f__B_SInt");
    }

    #[test]
    fn distinct_keys_get_distinct_names() {
        let keys: Vec<(&str, &str, Option<u32>, Vec<String>)> = vec![
            ("fn", "f", None, args(&["B", "Int"])),
            ("fn", "f_B", None, args(&["Int"])),
            ("fn", "f", None, args(&["B_Int"])),
            ("fn", "f", None, args(&["Pair[A, B]"])),
            ("fn", "f", None, args(&["Pair[A", "B]"])),
            ("fn", "f", None, args(&["Pair_LA_CB_R"])),
            ("fn", "f", None, args(&["Option[Vec[Int]]"])),
            ("fn", "f", None, args(&["Option[Vec_Int]"])),
            ("fn", "f", None, args(&["Option", "Vec[Int]"])),
            ("struct", "Box", None, args(&["Int"])),
            ("struct", "Box_SInt", None, args(&[])),
            ("enum", "Box", None, args(&["Int"])),
            ("enum", "E", None, args(&["Fn[Int, Bool]"])),
            ("enum", "E", None, args(&["Fn[Int]", "Bool"])),
            ("fn", "first", Some(12), args(&["Int"])),
            ("fn", "first__s12", None, args(&["Int"])),
            ("fn", "first", Some(1), args(&["2", "Int"])),
            ("fn", "first", None, args(&["(Int, Bool)"])),
            ("fn", "first", None, args(&["(Int", "Bool)"])),
        ];
        let mut seen = BTreeMap::new();
        for (kind, name, symbol, type_args) in &keys {
            let mangled = instantiation(kind, name, *symbol, type_args);
            assert!(
                mangled
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "{mangled}"
            );
            if let Some(previous) = seen.insert(mangled.clone(), (kind, name, symbol, type_args)) {
                panic!(
                    "{mangled} for both {previous:?} and {:?}",
                    (kind, name, symbol, type_args)
                );
            }
        }
    }

    #[test]
    fn names_are_deterministic_and_ignore_spaces() {
        let first = instantiation("fn", "map", None, &args(&["Map[String, Int]"]));
        for _ in 0..16 {
            assert_eq!(
                first,
                instantiation("fn", "map", None, &args(&["Map[String, Int]"]))
            );
        }
        assert_eq!(
            first,
            instantiation("fn", "map", None, &args(&["Map[String,Int]"]))
        );
        assert_eq!(first, "fn_map_SMap_LString_CInt_R");
    }

    #[test]
    fn join_escapes_each_part() {
        assert_ne!(
            join("aic_dynwrap", &["A_B", "c", "T"]),
            join("aic_dynwrap", &["A", "B_c", "T"])
        );
        assert_eq!(join("p", &["a.b"]), "p_Sa_x2E_b");
    }
}

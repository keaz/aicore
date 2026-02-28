pub(super) fn runtime_c_source() -> &'static str {
    concat!(
        include_str!("runtime/part01.c"),
        include_str!("runtime/part02.c"),
        include_str!("runtime/part03.c"),
        include_str!("runtime/part04.c"),
        include_str!("runtime/part05.c")
    )
}

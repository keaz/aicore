use std::fs;
use std::path::Path;

const INIT_MANIFEST: &str = "[package]\nname = \"sample\"\nmain = \"src/main.aic\"\n";

const INIT_MAIN: &str = r#"module sample.main;

import std.io;

fn maybe_even(x: Int) -> Option[Int] {
    if x % 2 == 0 {
    Some(x)
} else {
    None()
}
}

fn main() -> Int effects { io } capabilities { io } {
    let v = maybe_even(10);
    let out = match v {
    Some(n) => n,
    None => 0,
};
    print_int(out);
    0
}
"#;

pub fn init_project(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path)?;
    fs::create_dir_all(path.join("src"))?;
    fs::create_dir_all(path.join("examples"))?;
    fs::create_dir_all(path.join("docs"))?;
    fs::create_dir_all(path.join("tests"))?;

    fs::write(path.join("aic.toml"), INIT_MANIFEST)?;
    fs::write(path.join("src/main.aic"), INIT_MAIN)?;

    Ok(())
}

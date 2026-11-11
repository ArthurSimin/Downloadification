fn main() {
    println!("cargo:rerun-if-changed=ui/app.slint");
    println!("cargo:rerun-if-changed=ui/theme.slint");

    if std::env::var_os("CARGO_FEATURE_GUI").is_some() {
        let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
        slint_build::compile_with_config("ui/app.slint", config)
            .expect("failed to compile Slint UI (ui/app.slint)");
    }
}

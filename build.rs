use std::{collections::HashMap, path::PathBuf};

fn main() {
    let library = HashMap::from([(
        "lucide".to_string(),
        PathBuf::from(lucide_slint::get_slint_file_path().to_string()),
    )]);
    let config = slint_build::CompilerConfiguration::new().with_library_paths(library);
    slint_build::compile_with_config("./src/ui/app_window.slint", config)
        .expect("Slint build failed");
}
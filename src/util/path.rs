pub fn get_path(file_name: String) -> String {
    let path = std::env::current_dir()
        .expect("Failed to get current directory")
        .join("src")
        .join("files")
        .join(file_name);
}
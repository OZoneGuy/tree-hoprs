fn main() {
    // copy the completion file to the target directory
    let home = std::env::var("HOME").unwrap();
    let completion_file = format!("{}/.config/fish/completions/tree-hoprs.fish", home);
    std::fs::copy("templates/fish/completion.fish", completion_file).unwrap();
}

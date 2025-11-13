use std::{error::Error, process::Command};

fn main() {
    println!("Hello, world!");
}

pub fn generate_file_tree(
    project_root: &str,
    depth: usize,
    exclusions: &[&str],
) -> Result<String, Box<dyn Error + Send + Sync>> {
    // Construct the tree command with exclusions
    let mut command = Command::new("tree");
    command.arg("-L").arg(depth.to_string()).arg(".");

    // Add exclusions using the --prune option
    for exclusion in exclusions {
        command.arg("-I").arg(exclusion);
    }

    // Set the working directory to project_root
    command.current_dir(project_root);

    // Execute the command
    let output = command.output()?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use crate::generate_file_tree;

    #[test]
    fn test_generate_file_tree() {
        let project_root = "./";
        let depth = 4;
        let exclusions: Vec<&'static str> = vec![
            "*.md",
            "*.lock",
            "*.json",
            "*.gitignore",
            "target",
            "data",
            "conversations",
            "**/.git",
            "**/.monuth",
        ];

        let file_tree = generate_file_tree(project_root, depth, &exclusions);
        println!("File Tree:\n{}", file_tree.unwrap());
    }
}

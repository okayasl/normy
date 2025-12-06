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

    use normy::{all_langs, lang::get_lang_entry_by_code};

    use crate::generate_file_tree;

    #[test]
    fn lang_statistics() {
        use std::collections::{BTreeMap, BTreeSet};

        // Accumulators
        let mut casemap = BTreeMap::<&str, usize>::new();
        let mut fold = BTreeMap::<&str, usize>::new();
        let mut transliterate = BTreeMap::<&str, usize>::new();
        let mut precomposed = BTreeMap::<&str, usize>::new();
        let mut spacing_diacritics = BTreeMap::<&str, usize>::new();
        let mut peek_pairs = BTreeMap::<&str, usize>::new();
        let mut has_segment_rules = BTreeSet::<&str>::new();
        let mut needs_segmentation = BTreeSet::<&str>::new();
        let mut unigram_cjk = BTreeSet::<&str>::new();

        for &lang in all_langs() {
            let e = get_lang_entry_by_code(lang.code).unwrap();

            if e.has_case_map() {
                *casemap.entry(lang.code()).or_default() += e.case_map().len();
            }
            if e.has_fold_map() {
                *fold.entry(lang.code()).or_default() += e.fold_map().len();
            }
            if e.has_transliterate_map() {
                *transliterate.entry(lang.code()).or_default() += e.transliterate_map().len();
            }
            if e.has_pre_composed_to_base_map() {
                *precomposed.entry(lang.code()).or_default() += e.pre_composed_to_base_map().len();
            }
            if e.has_spacing_diacritics() {
                let cnt = e.spacing_diacritics_slice().unwrap_or(&[]).len();
                *spacing_diacritics.entry(lang.code()).or_default() += cnt;
            }
            if e.has_peek_pairs() {
                *peek_pairs.entry(lang.code()).or_default() += e.peek_pairs().len();
            }
            if e.has_segment_rules() {
                has_segment_rules.insert(lang.code());
            }
            if e.needs_segmentation() {
                needs_segmentation.insert(lang.code());
            }
            if e.needs_unigram_cjk() {
                unigram_cjk.insert(lang.code());
            }
        }

        macro_rules! print_stat {
            ($map:expr, $name:expr) => {{
                let total = $map.len();
                if total == 0 {
                    println!("{:<28} 0 languages", $name);
                } else {
                    let mut items = $map
                        .iter()
                        .map(|(code, cnt)| format!("{code}({cnt})"))
                        .collect::<Vec<_>>();
                    items.sort();
                    println!(
                        "{:<28} {} language{} → {}",
                        $name,
                        total,
                        if total == 1 { "" } else { "s" },
                        items.join(" ")
                    );
                }
            }};
        }

        macro_rules! print_set {
            ($set:expr, $name:expr) => {{
                let total = $set.len();
                if total == 0 {
                    println!("{:<28} 0 languages", $name);
                } else {
                    let mut codes = $set.iter().copied().collect::<Vec<_>>();
                    codes.sort();
                    println!(
                        "{:<28} {} language{} → {}",
                        $name,
                        total,
                        if total == 1 { "" } else { "s" },
                        codes.join(" ")
                    );
                }
            }};
        }

        println!("=== Normy Language Property Statistics ===\n");
        print_stat!(casemap, "casemap");
        print_stat!(fold, "fold");
        print_stat!(transliterate, "transliterate");
        print_stat!(precomposed, "precomposed_to_base");
        print_stat!(spacing_diacritics, "spacing_diacritics");
        print_set!(has_segment_rules, "segment_rules (any)");
        //print_set!(needs_segmentation, "needs_segmentation");
        print_set!(unigram_cjk, "unigram_cjk");
        print_stat!(peek_pairs, "peek_pairs");
    }

    #[test]
    fn test_generate_file_tree() {
        let project_root = "./";
        let depth = 4;
        let exclusions: Vec<&'static str> = vec![
            "*.md",
            "*.lock",
            "*.json",
            "*.gitignore",
            "*.txt",
            "target",
            "proptest-regressions",
            "data",
            "conversations",
            "**/.git",
            "**/.monuth",
        ];

        let file_tree = generate_file_tree(project_root, depth, &exclusions);
        println!("File Tree:\n{}", file_tree.unwrap());
    }
}

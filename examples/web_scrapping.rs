use std::{
    fs,
    io::{self, Read, Write},
};

use normy::{NORMALIZE_WHITESPACE_FULL, NormyBuilder, StripHtml};

static CORPUS_WIKIPEDIA_ROOT_PATH: &str = "./examples/corpus/html/wikipedia/";

fn main() {
    let normy_html_stripper = NormyBuilder::default()
        .add_stage(StripHtml)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .build();

    let example_name = "rust";

    let rust_wiki_page_html = load_wikipedia_html(example_name).unwrap();

    let result = normy_html_stripper.normalize(&rust_wiki_page_html).unwrap();
    println!("{:?}", result);
    let result_name = example_name.to_owned() + "_strip_only";
    save_results(&result_name, &result);

    println!("\n=== RESULTS ===");
    println!("Input:  {} bytes", rust_wiki_page_html.len());
    println!(
        "Output: {} bytes ({:.1}% compression)",
        result.len(),
        (1.0 - result.len() as f64 / rust_wiki_page_html.len() as f64) * 100.0
    );
    println!("Words:  {}", result.split_whitespace().count());
    println!("\nFirst 500 chars:\n{}\n", &result[..500.min(result.len())]);

    // Critical checks
    assert!(result.contains("Rust is a general-purpose"));
    assert!(result.contains("Graydon Hoare"));
    assert!(!result.contains('<'));
    assert!(!result.contains("<script"));

    println!("✅ VALIDATION PASSED!");
}

pub fn load_wikipedia_html(html_name: &str) -> io::Result<String> {
    let corpus_path = CORPUS_WIKIPEDIA_ROOT_PATH.to_owned() + html_name + ".html";
    println!("reading file from corpus: {:?}", corpus_path);
    let mut file = fs::File::open(corpus_path)?;
    let mut html = String::new();
    file.read_to_string(&mut html)?;
    Ok(html)
}

pub fn save_results(result_name: &str, result: &str) {
    let result_path = CORPUS_WIKIPEDIA_ROOT_PATH.to_owned() + result_name + ".txt";
    println!("writing result file to : {:?}", result_path);
    let mut file = fs::File::create(result_path).unwrap();
    file.write_all(result.as_bytes()).unwrap();
}

//! How a modern search engine (Meilisearch-style) normalizes documents and queries

use normy::{ENG, FRA, TUR, builder, profile::preset::search};

fn main() {
    let tr = builder().lang(TUR).build();
    let fr = builder().lang(FRA).build();
    let en = builder().lang(ENG).build();

    let text = "café naïve résumé İstanbul";

    println!("Input: {text}\n");

    println!(
        "→ As Turkish user: {}",
        tr.normalize_with_profile(&search(), text).unwrap()
    );
    // → "café naïve résumé istanbul" ← preserves French words!

    println!(
        "→ As French user : {}",
        fr.normalize_with_profile(&search(), text).unwrap()
    );
    // → "cafe naive resume istanbul" ← strips French diacritics

    println!(
        "→ As English user: {}",
        en.normalize_with_profile(&search(), text).unwrap()
    );
    // → "café naïve résumé istanbul" sane as Turkish
}

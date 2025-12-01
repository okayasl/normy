use proc_macro::TokenStream;
use quote::quote;
use std::collections::BTreeMap;
use syn::{
    Expr, Ident, Lit, LitBool, LitChar, LitStr, Result,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    token,
}; // Using BTreeMap internally to maintain sorting during parsing

/// Represents a single character mapping (e.g., 'A' => 'a' or 'ß' => "ss")
#[derive(Debug)]
struct Mapping {
    from: char,
    to_char: Option<char>,  // For case maps ('I' => 'ı')
    to_str: Option<String>, // For fold/transliterate maps ('ß' => "ss")
}

// Helper to parse the inner part of a mapping array: [ 'A' => 'a', ... ]
impl Parse for Mapping {
    fn parse(input: ParseStream) -> Result<Self> {
        let from: Lit = input.parse()?;
        let from = if let Lit::Char(c) = from {
            c.value()
        } else {
            return Err(input.error("expected char for 'from' value"));
        };
        input.parse::<token::FatArrow>()?; // '=>'

        let lookahead = input.lookahead1();
        let (to_char, to_str) = if lookahead.peek(LitChar) {
            let to = input.parse::<LitChar>()?.value();
            (Some(to), None)
        } else if lookahead.peek(LitStr) {
            let to = input.parse::<LitStr>()?.value();
            (None, Some(to))
        } else {
            return Err(input.error("expected char or string for 'to' value"));
        };

        Ok(Mapping {
            from,
            to_char,
            to_str,
        })
    }
}

// Struct to hold all data for a single language definition
#[derive(Debug)]
struct Language {
    code_ident: Ident,
    code_str: LitStr,
    name: LitStr,
    case_map: BTreeMap<char, char>,
    fold_map: BTreeMap<char, String>,
    transliterate_map: BTreeMap<char, String>,
    strip_map: BTreeMap<char, char>,
    spacing_diacritics: Vec<char>,
    needs_segmentation: LitBool,
    requires_peek_ahead: LitBool,
    peek_pairs: Vec<Expr>, // Using Expr for simplicity with complex structure
    segment_rules: Vec<Expr>,
    unigram_cjk: LitBool,
}

// Parses a field with an array value, e.g., `case: [ 'A' => 'a', ... ],`
fn parse_map_field<T, F>(
    input: ParseStream,
    field_name: &str,
    mut transform: F,
) -> Result<BTreeMap<char, T>>
where
    F: FnMut(Mapping) -> T,
{
    let field_ident: Ident = input.parse()?;
    if field_ident != field_name {
        return Err(input.error(format!("Expected field '{}'", field_name)));
    }
    input.parse::<token::Colon>()?; // :
    let content;
    syn::bracketed!(content in input); // [ ... ]

    let map_entries = Punctuated::<Mapping, token::Comma>::parse_terminated(&content)?;

    let mut map = BTreeMap::new();
    for mapping in map_entries {
        map.insert(mapping.from, transform(mapping));
    }

    input.parse::<token::Comma>()?; // ,

    Ok(map)
}

// Parses a field with a simple boolean value, e.g., `needs_word_segmentation: false,`
fn parse_bool_field(input: ParseStream, field_name: &str) -> Result<LitBool> {
    let field_ident: Ident = input.parse()?;
    if field_ident != field_name {
        return Err(input.error(format!("Expected field '{}'", field_name)));
    }
    input.parse::<token::Colon>()?;
    let value: LitBool = input.parse()?;
    input.parse::<token::Comma>()?;
    Ok(value)
}

// Parses a field with a char array, e.g., `spacing_diacritics: [ '̀', '́' ],`
fn parse_char_vec_field(input: ParseStream, field_name: &str) -> Result<Vec<char>> {
    let field_ident: Ident = input.parse()?;
    if field_ident != field_name {
        return Err(input.error(format!("Expected field '{}'", field_name)));
    }
    input.parse::<token::Colon>()?;
    let content;
    syn::bracketed!(content in input);
    let chars = Punctuated::<LitChar, token::Comma>::parse_terminated(&content)?
        .into_iter()
        .map(|lc| lc.value())
        .collect();
    input.parse::<token::Comma>()?;
    Ok(chars)
}

// Parses the definition for a single language block
impl Parse for Language {
    fn parse(input: ParseStream) -> Result<Self> {
        let code_ident: Ident = input.parse()?;
        input.parse::<token::Comma>()?;
        let code_str: LitStr = input.parse()?;
        input.parse::<token::Comma>()?;
        let name: LitStr = input.parse()?;
        input.parse::<token::Comma>()?;

        // 1. case: [ 'A' => 'a', ... ]
        let case_map = parse_map_field(input, "case", |m| {
            m.to_char.expect("Case map must map to a single char")
        })?;

        // 2. fold: [ 'ß' => "ss", ... ]
        let fold_map = parse_map_field(input, "fold", |m| {
            m.to_str.expect("Fold map must map to a string")
        })?;

        // 3. transliterate: [ 'Ä' => "ae", ... ]
        let transliterate_map = parse_map_field(input, "transliterate", |m| {
            m.to_str.expect("Transliterate map must map to a string")
        })?;

        // 4. precomposed_to_base: [ 'á' => 'a', ... ]
        let strip_map = parse_map_field(input, "precomposed_to_base", |m| {
            m.to_char.expect("Strip map must map to a single char")
        })?;

        // 5. spacing_diacritics: [ '̀', '́', ... ]
        let mut spacing_diacritics = parse_char_vec_field(input, "spacing_diacritics")?;
        spacing_diacritics.sort_unstable(); // Sort the simple char slice

        // 6. needs_word_segmentation: bool
        let needs_segmentation = parse_bool_field(input, "needs_word_segmentation")?;

        // 7. requires_peek_ahead: bool
        let requires_peek_ahead = parse_bool_field(input, "requires_peek_ahead")?;

        // 8. peek_pairs: []
        // For simplicity, we parse this as a Vec<Expr> since the structure is complex
        let peek_pairs = parse_expr_array_field(input, "peek_pairs")?;

        // 9. segment_rules: []
        let segment_rules = parse_expr_array_field(input, "segment_rules")?;

        // 10. unigram_cjk: bool
        let unigram_cjk = parse_bool_field(input, "unigram_cjk")?;

        Ok(Language {
            code_ident,
            code_str,
            name,
            case_map,
            fold_map,
            transliterate_map,
            strip_map,
            spacing_diacritics,
            needs_segmentation,
            requires_peek_ahead,
            peek_pairs,
            segment_rules,
            unigram_cjk,
        })
    }
}

// Parses fields that contain arbitrary Rust expressions in an array
fn parse_expr_array_field(input: ParseStream, field_name: &str) -> Result<Vec<Expr>> {
    let field_ident: Ident = input.parse()?;
    if field_ident != field_name {
        return Err(input.error(format!("Expected field '{}'", field_name)));
    }
    input.parse::<token::Colon>()?;
    let content;
    syn::bracketed!(content in input);

    // Parse the inner expressions separated by commas
    let exprs = Punctuated::<Expr, token::Comma>::parse_terminated(&content)?
        .into_iter()
        .collect();

    input.parse::<token::Comma>()?;
    Ok(exprs)
}

/// Parses the entire input stream, which is a list of language definitions
struct LanguagesInput {
    languages: Punctuated<Language, token::Comma>,
}

impl Parse for LanguagesInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let languages = Punctuated::<Language, token::Comma>::parse_terminated(input)?;
        Ok(LanguagesInput { languages })
    }
}

/// The main procedural macro entry point.
#[proc_macro]
pub fn define_languages(input: TokenStream) -> TokenStream {
    // We expect the input to look like: define_languages! { ... input ... }
    // The tokens passed to the proc macro *are* the inner tokens.
    let input = parse_macro_input!(input as LanguagesInput);
    let languages = input.languages.into_iter().collect::<Vec<_>>();

    generate_code(languages).into()
}

fn generate_code(languages: Vec<Language>) -> proc_macro2::TokenStream {
    let mut lang_consts = Vec::new();
    let mut modules = Vec::new();
    let mut phf_entries = Vec::new();

    // The main loop that iterates over each language and generates the code
    for lang in languages {
        let code = lang.code_ident;
        let code_str = lang.code_str;
        let name = lang.name;

        // --- 1. Compile-Time Sorting and Data Generation ---

        // Case Map: Vec<(char, char)>
        let mut case_map: Vec<_> = lang.case_map.into_iter().collect();
        case_map.sort_by_key(|k| k.0);
        let case_map_items = case_map
            .iter()
            .map(|(f, t)| quote! { CaseMap { from: #f, to: #t } });
        let has_case_map = !case_map.is_empty();

        // Fold Map: Vec<(char, String)>
        let mut fold_map: Vec<_> = lang.fold_map.into_iter().collect();
        fold_map.sort_by_key(|k| k.0);
        let fold_map_items = fold_map
            .iter()
            .map(|(f, t)| quote! { FoldMap { from: #f, to: #t } });
        let has_fold_map = !fold_map.is_empty();
        let fold_chars: Vec<char> = fold_map.iter().map(|m| m.0).collect();
        let has_one_to_one_folds = fold_map.iter().all(|(_, t)| t.chars().count() == 1);

        // Transliterate Map: Vec<(char, String)>
        let mut transliterate_map: Vec<_> = lang.transliterate_map.into_iter().collect();
        transliterate_map.sort_by_key(|k| k.0);
        let transliterate_items = transliterate_map
            .iter()
            .map(|(f, t)| quote! { FoldMap { from: #f, to: #t } });
        let has_transliterate_map = !transliterate_map.is_empty();
        let translit_chars: Vec<char> = transliterate_map.iter().map(|m| m.0).collect();
        let has_one_to_one_transliterate = transliterate_map
            .iter()
            .all(|(_, t)| t.chars().count() == 1);

        // Strip Map (Precomposed To Base): Vec<(char, char)>
        let mut strip_map: Vec<_> = lang.strip_map.into_iter().collect();
        strip_map.sort_by_key(|k| k.0);
        let strip_map_items = strip_map
            .iter()
            .map(|(f, t)| quote! { StripMap { from: #f, to: #t } });
        let has_strip_map = !strip_map.is_empty();
        let strip_chars: Vec<char> = strip_map.iter().map(|m| m.0).collect();

        // Spacing Diacritics: Vec<char> (already sorted in parser)
        let spacing_diacritics = lang.spacing_diacritics;
        let has_diacritics = !spacing_diacritics.is_empty();

        // Segment Rules and Peek Pairs (parsed as arbitrary expressions)
        let peek_pairs = lang.peek_pairs;
        let segment_rules = lang.segment_rules;
        let has_peek_pairs = !peek_pairs.is_empty();
        let has_segment_rules = !segment_rules.is_empty();

        // Boolean flags (parsed as LitBool)
        let needs_segmentation = lang.needs_segmentation;
        let requires_peek_ahead = lang.requires_peek_ahead;
        let unigram_cjk = lang.unigram_cjk;

        let mod_name_ident = syn::Ident::new(
            &format!("{}_data", code).to_lowercase(),
            proc_macro2::Span::call_site(),
        );

        // --- 2. Generate the Public Language Constant and Data Module ---

        lang_consts.push(quote! {
            pub const #code: Lang = Lang { code: #code_str, name: #name };
        });

        modules.push(quote! {
            mod #mod_name_ident {
                use super::*;

                // DATA ARRAYS (Now perfectly sorted for binary search)
                pub static CASE: &[CaseMap] = &[#(#case_map_items),*];
                pub static FOLD: &[FoldMap] = &[#(#fold_map_items),*];
                pub static TRANSLITERATE: &[FoldMap] = &[#(#transliterate_items),*];
                pub static PRECOMPOSED_TO_BASE: &[StripMap] = &[#(#strip_map_items),*];
                pub static SPACING_DIACRITICS: &[char] = &[#(#spacing_diacritics),*];

                pub static FOLD_CHAR_SLICE: &[char] = &[#(#fold_chars),*];
                pub static TRANSLITERATE_CHAR_SLICE: &[char] = &[#(#translit_chars),*];
                pub static STRIP_CHAR_SLICE: &[char] = &[#(#strip_chars),*];

                pub static PEEK_PAIRS: &[PeekPair] = &[#(#peek_pairs),*];
                pub static SEGMENT_RULES: &[SegmentRule] = &[#(#segment_rules),*];

                // PRECOMPUTED FLAGS (Calculated at compile time)
                pub const CODE: &str = #code_str;
                pub const NEEDS_WORD_SEGMENTATION: bool = #needs_segmentation;
                pub const REQUIRES_PEEK_AHEAD: bool = #requires_peek_ahead;
                pub const UNIGRAM_CJK: bool = #unigram_cjk;

                pub const HAS_CASE_MAP: bool = #has_case_map;
                pub const HAS_FOLD_MAP: bool = #has_fold_map;
                pub const HAS_TRANSLITERATE_MAP: bool = #has_transliterate_map;
                pub const HAS_STRIP_MAP: bool = #has_strip_map;
                pub const HAS_DIACRITICS: bool = #has_diacritics;
                pub const HAS_PEEK_PAIRS: bool = #has_peek_pairs;
                pub const HAS_SEGMENT_RULES: bool = #has_segment_rules;
                pub const HAS_ONE_TO_ONE_FOLDS: bool = #has_one_to_one_folds;
                pub const HAS_ONE_TO_ONE_TRANSLITERATE: bool = #has_one_to_one_transliterate;
            }
        });

        // --- 3. Prepare PHF Map Entry ---
        let mod_path = mod_name_ident;

        phf_entries.push(quote! {
            #code_str => LangEntry {
                has_case_map: #mod_path::HAS_CASE_MAP,
                has_fold_map: #mod_path::HAS_FOLD_MAP,
                has_transliterate_map: #mod_path::HAS_TRANSLITERATE_MAP,
                has_strip_map: #mod_path::HAS_STRIP_MAP,
                has_diacritics: #mod_path::HAS_DIACRITICS,
                has_peek_pairs: #mod_path::HAS_PEEK_PAIRS,
                has_segment_rules: #mod_path::HAS_SEGMENT_RULES,
                has_one_to_one_folds: #mod_path::HAS_ONE_TO_ONE_FOLDS,
                has_one_to_one_transliterate: #mod_path::HAS_ONE_TO_ONE_TRANSLITERATE,
                needs_segmentation: #mod_path::NEEDS_WORD_SEGMENTATION,
                requires_peek_ahead: #mod_path::REQUIRES_PEEK_AHEAD,
                unigram_cjk: #mod_path::UNIGRAM_CJK,

                code: #mod_path::CODE,
                case_map: #mod_path::CASE,
                fold_map: #mod_path::FOLD,
                transliterate_map: #mod_path::TRANSLITERATE,
                strip_map: #mod_path::PRECOMPOSED_TO_BASE,
                strip_char_slice: #mod_path::STRIP_CHAR_SLICE,
                // Using Option<&'static [char]> for diacritics based on emptiness
                diacritics: if #mod_path::SPACING_DIACRITICS.is_empty() { None } else { Some(#mod_path::SPACING_DIACRITICS) },
                diacritic_slice: if #mod_path::SPACING_DIACRITICS.is_empty() { None } else { Some(#mod_path::SPACING_DIACRITICS) },
                fold_char_slice: #mod_path::FOLD_CHAR_SLICE,
                transliterate_char_slice: #mod_path::TRANSLITERATE_CHAR_SLICE,
                peek_pairs: #mod_path::PEEK_PAIRS,
                segment_rules: #mod_path::SEGMENT_RULES,
            },
        });
    }

    // --- 4. Assemble Final Output ---

    // We assume the caller (your main crate) has defined the Lang, CaseMap, FoldMap, etc., structs.
    // The macro generates the constants, modules, and the PHF map.
    quote! {
        use crate::lang::{Lang, LangEntry, CaseMap, FoldMap, StripMap, PeekPair, SegmentRule};

        #(#lang_consts)*
        #(#modules)*

        paste::paste! {
            /// A fast, compile-time generated PHF map for looking up language data by 3-letter code.
            pub(crate) static LANG_TABLE: phf::Map<&'static str, LangEntry> = phf::phf_map! {
                #(#phf_entries)*
            };
        }

        // Helper functions
        pub fn from_code(code: &str) -> Option<&'static LangEntry> {
            LANG_TABLE.get(&code.to_ascii_uppercase())
        }

        pub const fn all_langs() -> &'static [Lang] {
             // You would need to collect all Lang constants here if required,
             // but for simplicity, the PHF map serves as the single source of truth.
             // This is often omitted or generated differently, but kept for context.
             &[]
        }
    }
}

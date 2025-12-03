// normy-lang-macros/src/lib.rs

use proc_macro::TokenStream;
use quote::quote;
use std::collections::BTreeMap;
use syn::{
    Expr, Ident, Lit, LitBool, LitChar, LitStr, Result,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    token,
};

/// Represents a single character mapping (e.g., 'A' => 'a' or 'ß' => "ss")
#[derive(Debug)]
struct Mapping {
    from: char,
    to_char: Option<char>,
    to_str: Option<String>,
}

impl Parse for Mapping {
    fn parse(input: ParseStream) -> Result<Self> {
        let from: Lit = input.parse()?;
        let from = if let Lit::Char(c) = from {
            c.value()
        } else {
            return Err(input.error("expected char for 'from' value"));
        };
        input.parse::<token::FatArrow>()?;

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
    peek_pairs: Vec<Expr>,
    segment_rules: Vec<Expr>,
    unigram_cjk: LitBool,
}

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
    input.parse::<token::Colon>()?;
    let content;
    syn::bracketed!(content in input);

    let map_entries = Punctuated::<Mapping, token::Comma>::parse_terminated(&content)?;

    let mut map = BTreeMap::new();
    for mapping in map_entries {
        map.insert(mapping.from, transform(mapping));
    }

    input.parse::<token::Comma>()?;
    Ok(map)
}

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

impl Parse for Language {
    fn parse(input: ParseStream) -> Result<Self> {
        let code_ident: Ident = input.parse()?;
        input.parse::<token::Comma>()?;
        let code_str: LitStr = input.parse()?;
        input.parse::<token::Comma>()?;
        let name: LitStr = input.parse()?;
        input.parse::<token::Comma>()?;

        let case_map = parse_map_field(input, "case", |m| {
            m.to_char.expect("Case map must map to a single char")
        })?;

        let fold_map = parse_map_field(input, "fold", |m| {
            m.to_str.expect("Fold map must map to a string")
        })?;

        let transliterate_map = parse_map_field(input, "transliterate", |m| {
            m.to_str.expect("Transliterate map must map to a string")
        })?;

        let strip_map = parse_map_field(input, "precomposed_to_base", |m| {
            m.to_char.expect("Strip map must map to a single char")
        })?;

        let mut spacing_diacritics = parse_char_vec_field(input, "spacing_diacritics")?;
        spacing_diacritics.sort_unstable();

        let needs_segmentation = parse_bool_field(input, "needs_word_segmentation")?;
        let requires_peek_ahead = parse_bool_field(input, "requires_peek_ahead")?;
        let peek_pairs = parse_expr_array_field(input, "peek_pairs")?;
        let segment_rules = parse_expr_array_field(input, "segment_rules")?;
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

fn parse_expr_array_field(input: ParseStream, field_name: &str) -> Result<Vec<Expr>> {
    let field_ident: Ident = input.parse()?;
    if field_ident != field_name {
        return Err(input.error(format!("Expected field '{}'", field_name)));
    }
    input.parse::<token::Colon>()?;
    let content;
    syn::bracketed!(content in input);

    let exprs = Punctuated::<Expr, token::Comma>::parse_terminated(&content)?
        .into_iter()
        .collect();

    input.parse::<token::Comma>()?;
    Ok(exprs)
}

struct LanguagesInput {
    languages: Punctuated<Language, token::Comma>,
}

impl Parse for LanguagesInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let languages = Punctuated::<Language, token::Comma>::parse_terminated(input)?;
        Ok(LanguagesInput { languages })
    }
}

#[proc_macro]
pub fn define_languages(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LanguagesInput);
    let languages = input.languages.into_iter().collect::<Vec<_>>();
    generate_code(languages).into()
}

/// Generate optimized find function for char->char mappings
fn generate_char_find_fn(
    fn_name: &str,
    map: &[(char, char)],
    data_name: &str,
) -> proc_macro2::TokenStream {
    let fn_ident = syn::Ident::new(fn_name, proc_macro2::Span::call_site());
    let data_ident = syn::Ident::new(data_name, proc_macro2::Span::call_site());
    if map.is_empty() {
        return quote! {
            #[inline(always)]
            pub const fn #fn_ident(_: char) -> Option<char> {
                None
            }
        };
    }
    if map.len() <= 4 {
        // Perfect unrolled match for tiny maps
        let arms = map.iter().map(|(from, to)| {
            quote! { #from => Some(#to) }
        });
        quote! {
            #[inline(always)]
            pub const fn #fn_ident(c: char) -> Option<char> {
                match c {
                    #(#arms,)*
                    _ => None,
                }
            }
        }
    } else if map.len() <= 15 {
        // Linear scan for medium maps (cache-friendly)
        let froms: Vec<_> = map.iter().map(|(f, _)| f).collect();
        let tos: Vec<_> = map.iter().map(|(_, t)| t).collect();
        quote! {
            #[inline(always)]
            pub fn #fn_ident(c: char) -> Option<char> {
                const FROMS: &[char] = &[#(#froms),*];
                const TOS: &[char] = &[#(#tos),*];
                FROMS.iter().position(|&x| x == c).map(|i| TOS[i])
            }
        }
    } else {
        // Binary search for large maps (now uses #data_ident)
        quote! {
            #[inline(always)]
            pub fn #fn_ident(c: char) -> Option<char> {
                match #data_ident.binary_search_by_key(&c, |m| m.from) {
                    Ok(i) => Some(#data_ident[i].to),
                    Err(_) => None,
                }
            }
        }
    }
}

/// Generate optimized find function for char->str mappings
fn generate_str_find_fn(
    fn_name: &str,
    map: &[(char, String)],
    data_name: &str,
) -> proc_macro2::TokenStream {
    let fn_ident = syn::Ident::new(fn_name, proc_macro2::Span::call_site());
    let data_ident = syn::Ident::new(data_name, proc_macro2::Span::call_site());

    if map.is_empty() {
        return quote! {
            #[inline(always)]
            pub const fn #fn_ident(_: char) -> Option<&'static str> {
                None
            }
        };
    }

    if map.len() <= 4 {
        // Perfect unrolled match
        let arms = map.iter().map(|(from, to)| {
            quote! { #from => Some(#to) }
        });
        quote! {
            #[inline(always)]
            pub const fn #fn_ident(c: char) -> Option<&'static str> {
                match c {
                    #(#arms,)*
                    _ => None,
                }
            }
        }
    } else if map.len() <= 15 {
        // Linear scan
        let froms: Vec<_> = map.iter().map(|(f, _)| f).collect();
        let tos: Vec<_> = map.iter().map(|(_, t)| t).collect();
        quote! {
            #[inline(always)]
            pub fn #fn_ident(c: char) -> Option<&'static str> {
                const FROMS: &[char] = &[#(#froms),*];
                const TOS: &[&str] = &[#(#tos),*];
                FROMS.iter().position(|&x| x == c).map(|i| TOS[i])
            }
        }
    } else {
        // Binary search
        quote! {
            #[inline(always)]
            pub fn #fn_ident(c: char) -> Option<&'static str> {
                match #data_ident.binary_search_by_key(&c, |m| m.from) {
                    Ok(i) => Some(#data_ident[i].to),
                    Err(_) => None,
                }
            }
        }
    }
}

/// Generate optimized contains check for char slices
fn generate_contains_fn(fn_name: &str, chars: &[char]) -> proc_macro2::TokenStream {
    let fn_ident = syn::Ident::new(fn_name, proc_macro2::Span::call_site());

    if chars.is_empty() {
        return quote! {
            #[inline(always)]
            pub const fn #fn_ident(_: char) -> bool {
                false
            }
        };
    }

    if chars.len() <= 4 {
        // Perfect unrolled match
        let arms = chars.iter().map(|c| {
            quote! { #c }
        });
        quote! {
            #[inline(always)]
            pub const fn #fn_ident(c: char) -> bool {
                matches!(c, #(#arms)|*)
            }
        }
    } else if chars.len() <= 15 {
        // Linear scan
        quote! {
            #[inline(always)]
            pub fn #fn_ident(c: char) -> bool {
                const CHARS: &[char] = &[#(#chars),*];
                CHARS.contains(&c)
            }
        }
    } else {
        // Binary search
        quote! {
            #[inline(always)]
            pub fn #fn_ident(c: char) -> bool {
                const CHARS: &[char] = &[#(#chars),*];
                CHARS.binary_search(&c).is_ok()
            }
        }
    }
}

/// Generate CharMap constructor based on size
fn generate_char_map_constructor(
    map: &[(char, char)],
    mod_name: &syn::Ident,
    data_name: &str,
) -> proc_macro2::TokenStream {
    let len = map.len();
    let data_ident = syn::Ident::new(data_name, proc_macro2::Span::call_site());

    if len == 0 {
        quote! { CharMap::Empty(EmptyCharMap) }
    } else if len <= 4 {
        quote! { CharMap::Tiny(TinyCharMap::new(#mod_name::#data_ident)) }
    } else if len <= 15 {
        quote! { CharMap::Small(SmallCharMap::new(#mod_name::#data_ident)) }
    } else {
        quote! { CharMap::Binary(BinaryCharMap::new(#mod_name::#data_ident)) }
    }
}

/// Generate StrMap constructor based on size
fn generate_str_map_constructor(
    map: &[(char, String)],
    mod_name: &syn::Ident,
    data_name: &str,
) -> proc_macro2::TokenStream {
    let len = map.len();
    let data_ident = syn::Ident::new(data_name, proc_macro2::Span::call_site());

    if len == 0 {
        quote! { StrMap::Empty(EmptyStrMap) }
    } else if len <= 4 {
        quote! { StrMap::Tiny(TinyStrMap::new(#mod_name::#data_ident)) }
    } else if len <= 15 {
        quote! { StrMap::Small(SmallStrMap::new(#mod_name::#data_ident)) }
    } else {
        quote! { StrMap::Binary(BinaryStrMap::new(#mod_name::#data_ident)) }
    }
}

/// Generate CharSet constructor based on size
fn generate_char_set_constructor(
    chars: &[char],
    mod_name: &syn::Ident,
    data_name: &str,
) -> proc_macro2::TokenStream {
    let len = chars.len();
    let data_ident = syn::Ident::new(data_name, proc_macro2::Span::call_site());

    if len == 0 {
        quote! { CharSet::Empty }
    } else if len <= 4 {
        quote! { CharSet::Tiny(TinyCharSet::new(#mod_name::#data_ident)) }
    } else if len <= 15 {
        quote! { CharSet::Small(SmallCharSet::new(#mod_name::#data_ident)) }
    } else {
        quote! { CharSet::Binary(BinaryCharSet::new(#mod_name::#data_ident)) }
    }
}

fn generate_code(languages: Vec<Language>) -> proc_macro2::TokenStream {
    let mut lang_consts = Vec::new();
    let mut modules = Vec::new();
    let mut phf_entries = Vec::new();

    for lang in languages {
        let code = lang.code_ident;
        let code_str = lang.code_str;
        let name = lang.name;

        // Sort and collect maps
        let mut case_map: Vec<_> = lang.case_map.into_iter().collect();
        case_map.sort_by_key(|k| k.0);
        let has_case_map = !case_map.is_empty();

        let mut fold_map: Vec<_> = lang.fold_map.into_iter().collect();
        fold_map.sort_by_key(|k| k.0);
        let has_fold_map = !fold_map.is_empty();
        let fold_chars: Vec<char> = fold_map.iter().map(|m| m.0).collect();
        let has_one_to_one_folds = fold_map.iter().all(|(_, t)| t.chars().count() == 1);

        let mut transliterate_map: Vec<_> = lang.transliterate_map.into_iter().collect();
        transliterate_map.sort_by_key(|k| k.0);
        let has_transliterate_map = !transliterate_map.is_empty();
        let translit_chars: Vec<char> = transliterate_map.iter().map(|m| m.0).collect();
        let has_one_to_one_transliterate = transliterate_map
            .iter()
            .all(|(_, t)| t.chars().count() == 1);

        let mut strip_map: Vec<_> = lang.strip_map.into_iter().collect();
        strip_map.sort_by_key(|k| k.0);
        let has_strip_map = !strip_map.is_empty();
        let strip_chars: Vec<char> = strip_map.iter().map(|m| m.0).collect();

        let spacing_diacritics = lang.spacing_diacritics;
        let has_diacritics = !spacing_diacritics.is_empty();

        let peek_pairs = lang.peek_pairs;
        let segment_rules = lang.segment_rules;
        let has_peek_pairs = !peek_pairs.is_empty();
        let has_segment_rules = !segment_rules.is_empty();

        let needs_segmentation = lang.needs_segmentation;
        let requires_peek_ahead = lang.requires_peek_ahead;
        let unigram_cjk = lang.unigram_cjk;

        // Generate static data
        // produce owned token lists (so they can be reused / cloned)
        let case_map_items = case_map
            .iter()
            .map(|(f, t)| quote! { CaseMap { from: #f, to: #t } });
        let case_map_items_clone = case_map_items.clone();

        let fold_map_items = fold_map
            .iter()
            .map(|(f, t)| quote! { FoldMap { from: #f, to: #t } });
        let fold_map_items_clone = fold_map_items.clone();

        let transliterate_items = transliterate_map
            .iter()
            .map(|(f, t)| quote! { FoldMap { from: #f, to: #t } });
        let transliterate_items_clone = transliterate_items.clone();

        let strip_map_items = strip_map
            .iter()
            .map(|(f, t)| quote! { StripMap { from: #f, to: #t } });
        let strip_map_items_clone = strip_map
            .iter()
            .map(|(f, t)| quote! { CaseMap { from: #f, to: #t } });

        //

        // Generate optimized find functions
        let find_case = generate_char_find_fn("find_case_map", &case_map, "CASE");
        let find_fold = generate_str_find_fn("find_fold_map", &fold_map, "FOLD");
        let find_translit = generate_str_find_fn(
            "find_transliterate_map",
            &transliterate_map,
            "TRANSLITERATE",
        );
        let find_strip = generate_char_find_fn("find_strip_map", &strip_map, "PRECOMPOSED_TO_BASE");
        let contains_fold = generate_contains_fn("contains_fold_char", &fold_chars);
        let contains_translit =
            generate_contains_fn("contains_transliterate_char", &translit_chars);
        let contains_strip = generate_contains_fn("contains_strip_char", &strip_chars);
        let contains_diacritic = generate_contains_fn("contains_diacritic", &spacing_diacritics);

        let mod_name_ident = syn::Ident::new(
            &format!("{}_data", code).to_lowercase(),
            proc_macro2::Span::call_site(),
        );

        // Generate specialized map constructors with correct module path
        let case_map_constructor =
            generate_char_map_constructor(&case_map, &mod_name_ident, "CASE_DATA");
        let fold_map_constructor =
            generate_str_map_constructor(&fold_map, &mod_name_ident, "FOLD_DATA");
        let translit_map_constructor =
            generate_str_map_constructor(&transliterate_map, &mod_name_ident, "TRANSLITERATE_DATA");
        let strip_map_constructor =
            generate_char_map_constructor(&strip_map, &mod_name_ident, "STRIP_DATA");

        let fold_chars_constructor =
            generate_char_set_constructor(&fold_chars, &mod_name_ident, "FOLD_CHARS_DATA");
        let translit_chars_constructor = generate_char_set_constructor(
            &translit_chars,
            &mod_name_ident,
            "TRANSLITERATE_CHARS_DATA",
        );
        let strip_chars_constructor =
            generate_char_set_constructor(&strip_chars, &mod_name_ident, "STRIP_CHARS_DATA");
        let diacritics_constructor =
            generate_char_set_constructor(&spacing_diacritics, &mod_name_ident, "DIACRITICS_DATA");

        lang_consts.push(quote! {
            pub const #code: Lang = Lang { code: #code_str, name: #name };
        });

        modules.push(quote! {
            mod #mod_name_ident {
                use super::*;

                // Static data arrays (sorted for binary search)
                pub static CASE: &[CaseMap] = &[#(#case_map_items),*];
                pub static FOLD: &[FoldMap] = &[#(#fold_map_items),*];
                pub static TRANSLITERATE: &[FoldMap] = &[#(#transliterate_items),*];
                pub static PRECOMPOSED_TO_BASE: &[StripMap] = &[#(#strip_map_items),*];
                pub static SPACING_DIACRITICS: &[char] = &[#(#spacing_diacritics),*];

                pub static FOLD_CHAR_SLICE: &[char] = &[#(#fold_chars),*];
                pub static TRANSLITERATE_CHAR_SLICE: &[char] = &[#(#translit_chars),*];
                pub static STRIP_CHAR_SLICE: &[char] = &[#(#strip_chars),*];

                // new data
                pub static CASE_DATA: &[CaseMap] = &[#(#case_map_items_clone),*];
                pub static FOLD_DATA: &[FoldMap] = &[#(#fold_map_items_clone),*];
                pub static TRANSLITERATE_DATA: &[FoldMap] = &[#(#transliterate_items_clone),*];
                pub static STRIP_DATA: &[CaseMap] = &[#(#strip_map_items_clone),*];

                pub static FOLD_CHARS_DATA: &[char] = &[#(#fold_chars),*];
                pub static TRANSLITERATE_CHARS_DATA: &[char] = &[#(#translit_chars),*];
                pub static STRIP_CHARS_DATA: &[char] = &[#(#strip_chars),*];
                pub static DIACRITICS_DATA: &[char] = &[#(#spacing_diacritics),*];


                pub static PEEK_PAIRS: &[PeekPair] = &[#(#peek_pairs),*];
                pub static SEGMENT_RULES: &[SegmentRule] = &[#(#segment_rules),*];

                // Compile-time constants
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

                // Optimized lookup functions (zero runtime branching)
                #find_case
                #find_fold
                #find_translit
                #find_strip

                #contains_fold
                #contains_translit
                #contains_strip
                #contains_diacritic
            }
        });

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

                // Use specialized map constructors
                case_map_enum: #case_map_constructor,
                fold_map_enum: #fold_map_constructor,
                transliterate_map_enum: #translit_map_constructor,
                strip_map_enum: #strip_map_constructor,
                fold_chars_enum: #fold_chars_constructor,
                transliterate_chars_enum: #translit_chars_constructor,
                strip_chars_enum: #strip_chars_constructor,
                diacritics_enum: #diacritics_constructor,


                code: #mod_path::CODE,
                case_map: #mod_path::CASE,
                fold_map: #mod_path::FOLD,
                transliterate_map: #mod_path::TRANSLITERATE,
                strip_map: #mod_path::PRECOMPOSED_TO_BASE,
                strip_char_slice: #mod_path::STRIP_CHAR_SLICE,
                diacritics: if #mod_path::SPACING_DIACRITICS.is_empty() { None } else { Some(#mod_path::SPACING_DIACRITICS) },
                //diacritic_slice: if #mod_path::SPACING_DIACRITICS.is_empty() { None } else { Some(#mod_path::SPACING_DIACRITICS) },
                fold_char_slice: #mod_path::FOLD_CHAR_SLICE,
                transliterate_char_slice: #mod_path::TRANSLITERATE_CHAR_SLICE,
                peek_pairs: #mod_path::PEEK_PAIRS,
                segment_rules: #mod_path::SEGMENT_RULES,

                // Function pointers for zero-cost dispatch
                find_case_map: #mod_path::find_case_map,
                find_fold_map: #mod_path::find_fold_map,
                find_transliterate_map: #mod_path::find_transliterate_map,
                find_strip_map: #mod_path::find_strip_map,
                contains_fold_char: #mod_path::contains_fold_char,
                contains_transliterate_char: #mod_path::contains_transliterate_char,
                contains_strip_char: #mod_path::contains_strip_char,
                contains_diacritic: #mod_path::contains_diacritic,
            },
        });
    }

    quote! {
        use crate::lang::{Lang, LangEntry, CaseMap, FoldMap, StripMap, PeekPair, SegmentRule};

        #(#lang_consts)*
        #(#modules)*

        paste::paste! {
            pub(crate) static LANG_TABLE: phf::Map<&'static str, LangEntry> = phf::phf_map! {
                #(#phf_entries)*
            };
        }

        pub fn from_code(code: &str) -> Option<&'static LangEntry> {
            LANG_TABLE.get(&code.to_ascii_uppercase())
        }

        pub const fn all_langs() -> &'static [Lang] {
             &[]
        }
    }
}

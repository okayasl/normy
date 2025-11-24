# NORMY LANGUAGE PROPERTY RULES

## The Normative Authority on Language Behavior in Normy

> These rules are based exclusively on **linguistic, orthographic, and historical reality** — never on modern ASCII convenience, keyboard limitations, or search-engine compatibility inventions.

---

## RULE 1: `case_map` — Locale-Specific Case Conversions

Populate **only** when Unicode default case mapping is **linguistically incorrect**.

| Language             | Include? | Examples                     | Reason |
|----------------------|----------|------------------------------|--------|
| Turkish, Azerbaijani | Yes      | İ→i, I→ı                     | Dotted/dotless I distinction |
| Lithuanian           | Yes      | Preserves ogonek/accent contextually | Titlecase/lowercase differ |
| Catalan              | Yes      | l·l → L·L (preserves middle dot) | Orthographic rule |
| All others           | No       | —                            | Unicode default is correct |

> **Titlecasing**: Normy does **not** perform titlecasing. This is intentionally left to Unicode default or user code.

---

## RULE 2: `fold` — Linguistic Search-Equivalence Folding

Populate **only** for **official**, **native**, one-to-many equivalences used **within the language itself**.

| Language | Mapping                          | Include? | Justification |
|----------|----------------------------------|----------|-------------|
| German   | ß→"ss", ẞ→"ss"                   | Yes      | Official spelling reform — applies to both lowercase and uppercase |
| Dutch    | Ĳ→"ij", ĳ→"ij" (U+0132/U+0133 only) | Yes   | Ligature expands in native Dutch search |
| All others | —                              | No       | Not linguistically native |

> **Critical**:
>
> - `fold` applies **only** to precomposed ligature characters U+0132 (Ĳ) and U+0133 (ĳ).
> - The sequence `I + J` (or `i + j`) is **never** treated as a ligature in `fold`.  
> - Normy never infers linguistic ligatures from separate codepoints.  
> - Dutch titlecasing of IJ (e.g., "IJsselmeer") is **not** performed — left to user logic.

---

## RULE 3: `transliterate` — Historical Pre-Computer ASCII Conventions (Lossy)

Populate **only** when documented in **official pre-1980** contexts.

| Language       | Mapping                          | Include? | Historical Evidence |
|----------------|----------------------------------|----------|---------------------|
| German         | Ä→"ae", Ö→"oe", Ü→"ue", ß→"ss"   | Yes      | Reichsbahn, passports |
| Scandinavian   | Å→"aa", Ä→"ae", Ö→"oe"           | Yes      | Postal standards |
| Icelandic      | Þ→"th", Ð→"d"                    | Yes      | International naming |
| All others     | —                                | No       | No historical tradition |

> Strip removes marks; transliterate replaces letters. They never conflict — **transliterate always wins**.

---

## RULE 4: `precomposed_to_base` — Accent-Insensitive Search (Opt-In, Lossy, User Convenience Only)

Populate **only** when marks are pronunciation-based and commonly omitted.

| Language Group                            | Strip?         | Notes |
|-------------------------------------------|----------------|-------|
| French, Spanish, Portuguese, Italian, Catalan | Yes            | Accents routinely dropped |
| Vietnamese                                | Yes (practical) | Removes **both tone marks** and **vowel quality marks** (ơ→o, ư→u, â→a, ă→a, ô→o, ê→e) — **always destructive and meaning-destroying**. Normy does **not** attempt to preserve vowel class or shortness. Provided only because Vietnamese search engines universally support toneless search. |
| Czech, Slovak, Polish, Croatian, **Slovene** | Yes (practical) | Acute/caron are phonemic — **stripping destroys meaning** (e.g., c↔č, l↔ł, đ↔d). Included **only** due to overwhelming international search-engine practice, **not** linguistic validity. Slovene stripping is common but not universally expected. |
| German, Dutch, Nordic, Icelandic          | No             | Ä, Ĳ, Å, Æ, Ø are **distinct letters** |
| Turkish, Hungarian, Romanian              | No             | ğ, ş, ț are distinct phonemes |

> **Spanish ñ/Ñ must never be stripped** — it is a separate letter.  
> By default, `precomposed_to_base` is **disabled**. Normy is **non-destructive** out of the box.

---

## RULE 5: `spacing_diacritics` — Standalone Combining Marks Only

Populate **only** with marks that **never** form precomposed characters in NFC.

| Script         | Example Marks                       | Include? | Reason |
|----------------|-------------------------------------|----------|--------|
| Arabic         | fatḥa (U+064E), kasra (U+0650), ḍamma (U+064F) | Yes      | Optional vowel points — standalone only |
| Hebrew         | qamats (U+05B8), patach (U+05B7)    | Yes      | Optional pointing |
| Thai, Lao      | mai ek (U+0E48), sara a (U+0E32)    | Yes      | Standalone tone/vowel signs |
| Devanagari     | candrabindu (U+0901), nuktā (U+093C) | Yes only if never precomposed | Must verify per-script |
| Latin/Cyrillic | U+0300–U+036F combining diacritics  | No       | Form é, č, ą in NFC |

> `spacing_diacritics` removal **must never** delete any mark that would normally be precomposed in NFC.

---

## RULE 6: `needs_word_segmentation` — Script Boundary Segmentation

| Language       | needs_segmentation | unigram_cjk | Heuristic Details |
|----------------|--------------------|-------------|-------------------|
| Chinese (ZH)   | true               | true        | Full unigram breaking |
| Japanese (JA)  | true               | false       | Boundaries only at script transitions |
| Korean (KO)    | true               | false       | Same as Japanese |
| Thai, Lao, Khmer | true             | false       | Lightweight zero-width space insertion at script boundaries and legal syllable breaks — **no dictionary**, **no illegal cluster rejection** |
| Myanmar        | true               | false       | Lightweight syllable boundary heuristic — **no dictionary** |

---

## RULE 7: `requires_peek_ahead` — Multi-Character Contextual Folding

| Language | Rule             | Include? | Reason |
|----------|------------------|----------|--------|
| Dutch    | I + J → "ij"     | Yes      | Only known multi-character fold in Normy |
| All others | —              | No       | Dictionary-based logic not supported |

> Normy treats all other digraph letters (Czech “ch”, Slovak “dz/dž”, Croatian “lj/nj”, etc.) as **ordinary sequences** — they are **not** treated as atomic units because Unicode does not encode them as single codepoints.

---

## RULE 8: `format_scope` — Structured Text Normalization (Format-Aware)

| Scope                            | Normalize? | Justification |
|----------------------------------|------------|-----------|
| Text nodes                       | Yes        | Primary content |
| HTML `<script>`, `<style>`, `<pre>`, `<code>` | No  | Prevents code corruption |
| Markdown code blocks/fences/inline | No       | Preserves syntax |
| HTML attribute values            | No         | Ensures functionality |

---

## Out of Scope: Unidecode-Style ASCII Fallback

Normy **must never** include full Unidecode-style fallbacks (e.g., ğ→g, č→c, ø→o, ł→l).  
These belong to a **separate, optional compatibility module** (`normy-compat-ascii`), which is:

- Not part of linguistic normalization
- Not enabled by any default profile
- Explicitly opt-in for legacy systems

---

## Conflict Resolution Order

```text
1. NFC (always first)
2. case_map
3. fold
4. precomposed_to_base (opt-in)
5. transliterate (opt-in) → overrides precomposed_to_base
6. spacing_diacritics removal
7. normalize_whitespace
8. segment

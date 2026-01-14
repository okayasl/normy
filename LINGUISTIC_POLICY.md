# NORMY LANGUAGE PROPERTY RULES

## The Normative Authority on Language Behavior in Normy

> These rules are based exclusively on **linguistic, orthographic, and historical reality** — never on modern ASCII convenience, keyboard limitations, or search-engine compatibility inventions.

---

## Quick Reference: What Gets Modified

| Operation     | Languages                    | Lossy? | Default? | Example        |
| ------------- | ---------------------------- | ------ | -------- | -------------- |
| Case mapping  | Turkish, Lithuanian, Catalan | No     | Yes      | İ→i (Turkish)  |
| Folding       | German, Dutch                | No*    | Yes      | ß→"ss"         |
| Strip accents | Romance, Slavic, Vietnamese  | Yes    | **No**   | é→e            |
| Transliterate | German, Nordic, Russian      | Yes    | **No**   | Ä→"ae"         |
| Segment       | CJK, Indic, SEAsian          | No     | Yes      | "你好"→"你 好" |
| Remove marks  | Arabic, Hebrew               | Yes    | **No**   | fatḥa removed  |

\* *Folding is technically lossy (ß→"ss" is not reversible) but linguistically valid for search.*
\* *If a character is phonemically or orthographically meaningful in native text, it MUST NOT appear in precomposed_to_base. Inclusion signals accepted semantic loss.*

---

## RULE 1: `case_map` — Locale-Specific Case Conversions

Populate **only** when Unicode default case mapping is **linguistically incorrect**.

| Language             | Include? | Examples                             | Reason                       |
| -------------------- | -------- | ------------------------------------ | ---------------------------- |
| Turkish, Azerbaijani | Yes      | İ→i, I→ı                             | Dotted/dotless I distinction |
| Lithuanian           | Yes      | Preserves ogonek/accent contextually | Titlecase/lowercase differ   |
| All others           | No       | —                                    | Unicode default is correct   |

> **Titlecasing**: Normy does **not** perform titlecasing. This is intentionally left to Unicode default or user code.

---

## RULE 2: `fold` — Linguistic Search-Equivalence Folding

Populate **only** for **official**, **native**, one-to-many equivalences used **within the language itself**.

| Language   | Mapping                             | Include? | Justification                                                      |
| ---------- | ----------------------------------- | -------- | ------------------------------------------------------------------ |
| German     | ß→"ss", ẞ→"ss"                      | Yes      | Official spelling reform — applies to both lowercase and uppercase |
| Dutch      | Ĳ→"ij", ĳ→"ij" (U+0132/U+0133 only) | Yes      | Ligature expands in native Dutch search                            |
| All others | —                                   | No       | Not linguistically native                                          |

> **Critical**:
>
> - `fold` applies **only** to precomposed ligature characters U+0132 (Ĳ) and U+0133 (ĳ).
> - Both U+0132 (Ĳ) and U+0133 (ĳ) folds to **"ij"** (two lowercase letters)
> - The sequence `I + J` (or `i + j`) is **never** treated as a ligature in `fold`.  
> - Normy never infers linguistic ligatures from separate codepoints.  
> - Dutch titlecasing of IJ (e.g., "IJsselmeer") is **not** performed — left to user logic.
> - Only precomposed ligature characters are folded; sequences of separate codepoints are never treated as ligatures.

---

## RULE 3: `transliterate` — Historical Pre-Computer ASCII Conventions (Lossy)

Populate **only** when documented in **official pre-1980** contexts.

| Language     | Mapping                          | Include? | Historical Evidence                                         |
| ------------ | -------------------------------- | -------- | ----------------------------------------------------------- |
| German       | Ä→"ae", Ö→"oe", Ü→"ue", ß→"ss"   | Yes      | Reichsbahn, passports                                       |
| Scandinavian | Å→"aa", Ä→"ae", Ö→"oe"           | Yes      | Postal standards                                            |
| Icelandic    | Þ→"th", Ð→"d"                    | Yes      | International naming                                        |
| Russian      | ISO/R 9:1968 (see details below) | Yes      | ISO/R 9:1968 Scientific Transliteration (Pre-1980 standard) |
| All others   | —                                | No       | No historical tradition                                     |

**Russian Transliteration Examples (ISO/R 9:1968)**:

- Ю → "ju"
- Щ → "šč"  
- Ъ → "ʺ" (hard sign to modifier letter double prime)
- Ь → "ʹ" (soft sign to modifier letter prime)
- Я → "ja"
- Ч → "č"

> Strip removes marks; transliterate replaces letters. They never conflict — **transliterate always wins**.
> Transliteration is distinct from diacritic stripping; always overrides precomposed_to_base.

---

## RULE 4: `precomposed_to_base` — Accent-Insensitive Search (Opt-In, Lossy, User Convenience Only)

Populate **only** when marks are pronunciation-based and commonly omitted.

| Language Group                               | Strip?          | Notes                                                                                                                                                                                                                                                                                                                                                    |
| -------------------------------------------- | --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| French, Portuguese, Italian, Catalan         | Yes             | Accents routinely dropped                                                                                                                                                                                                                                                                                                                                |
| Spanish (except ñ/Ñ)                         | Yes             | Accents dropped, but **ñ/Ñ is a distinct letter** — never stripped                                                                                                                                                                                                                                                                                       |
| Vietnamese                                   | Yes (practical) | Removes **both tone marks** (◌́, ◌̀, ◌̉, ◌̃, ◌̣) and **vowel quality marks**. **Order matters**: decompose (NFD) → remove tone marks → remove quality marks (ơ→o, ư→u, â→a, ă→a, ô→o, ê→e) → recompose (NFC). **Always destructive and meaning-destroying**. Provided only because Vietnamese search engines universally support toneless search.             |
| Czech, Slovak, Polish, Croatian, **Slovene** | Yes (practical) | Acute/caron are phonemic — **stripping destroys meaning** (e.g., c↔č, l↔ł, đ↔d). Included **only** due to overwhelming international search-engine practice, **not** linguistic validity. **WARNING**: Native speakers expect diacritics in search. Use only for international/legacy systems. Slovene stripping is common but not universally expected. |
| German, Dutch, Nordic, Icelandic             | No              | Ä, Ĳ, Å, Æ, Ø are **distinct letters**                                                                                                                                                                                                                                                                                                                   |
| Turkish, Hungarian, Romanian                 | No              | ğ, ş, ț are distinct phonemes                                                                                                                                                                                                                                                                                                                            |

> **Spanish ñ/Ñ must never be stripped** — it is a separate letter in the Spanish alphabet, not an accented N.
> Any character that is phonemically or orthographically meaningful in native text must never appear here. Inclusion implies acceptance of semantic loss.  
> By default, `precomposed_to_base` is **disabled**. Normy is **non-destructive** out of the box.

---

## RULE 5: `spacing_diacritics` — Standalone Combining Marks Only

Populate **only** with marks that **never** form precomposed characters in NFC.

| Script         | Example Marks                                  | Include?                      | Reason                                                        |
| -------------- | ---------------------------------------------- | ----------------------------- | ------------------------------------------------------------- |
| Arabic         | fatḥa (U+064E), kasra (U+0650), ḍamma (U+064F) | Yes                           | Optional vowel points — standalone only                       |
| Arabic         | **shadda (U+0651)**                            | **No**                        | Gemination mark — **phonemically significant**, must preserve |
| Hebrew         | qamats (U+05B8), patach (U+05B7)               | Yes                           | Optional pointing                                             |
| Thai, Lao      | mai ek (U+0E48), sara a (U+0E32)               | Yes                           | Standalone tone/vowel signs                                   |
| Devanagari     | candrabindu (U+0901), nuktā (U+093C)           | Yes only if never precomposed | Must verify per-script                                        |
| Latin/Cyrillic | U+0300–U+036F combining diacritics             | No                            | Form é, č, ą in NFC                                           |
| Latin          | U+00B7 (MIDDLE DOT) in Catalan "l·l"           | No                            | Orthographic punctuation, not a diacritic — must preserve     |

> `spacing_diacritics` removal **must never** delete any mark that would normally be precomposed in NFC.

**Why Arabic shadda must be preserved**:

- Arabic vowel points (fatḥa, kasra, damma, tanwin, etc.): Yes
- Shadda (U+0651): **Explicitly excluded** — phonemically significant (gemination)
- Removing it fundamentally changes pronunciation and meaning
- Unlike vowel points (fatḥa, kasra, ḍamma), shadda is not optional in vocalized text
- Only pure spacing diacritics appear here; any phonemically meaningful mark is excluded.
- Maddah (U+0653), Hamza above/below: Include only if purely optional (current: included—justify or remove)

---

## RULE 6: `needs_word_segmentation` — Script Boundary Segmentation

| Language                  | needs_segmentation | unigram_cjk | Heuristic Details                                          |
| ------------------------- | ------------------ | ----------- | ---------------------------------------------------------- |
| Chinese (ZH)              | true               | true        | Full unigram breaking                                      |
| Japanese (JA)             | true               | false       | Boundaries only at script transitions                      |
| Korean (KO)               | true               | false       | Boundaries only at script transitions                      |
| Hindi (HI)                | true               | false       | ZWSP at virama+consonant with conjunct exception (र/य/व/ह) |
| Tamil (TA)                | true               | false       | ZWSP at virama+consonant (no exceptions)                   |
| Thai, Lao, Khmer, Myanmar | true               | false       | Script transitions only — no syllable breaking             |

**Performance Guarantee**: Languages with `needs_segmentation = false` incur **zero overhead**
**Indic Script Details**:

- **ZWSP Insertion**: Zero-Width Space (U+200B) inserted after virama when followed by consonant
- **Hindi Exception**: Prevents ZWSP for conjunct-forming consonants र (ra), य (ya), व (va), ह (ha)
  - Example: "विद्वत्" → "विद्वत्" (preserved)
  - Example: "पत्नी" → "पत्\u{200B}नी" (ZWSP inserted)
- **Other Indic Scripts**: Universal virama rule applies (no exceptions)
  - Example (Tamil): "பற்றி" → "பற்\u{200B}றி"

**Southeast Asian Scripts**: No syllable-level segmentation (would require dictionaries). Use specialized tokenizers (PyThaiNLP, LaoNLP, etc.) for true word/syllable breaking.

---

## RULE 8: `format_scope` — Structured Text Normalization (Format-Aware)

| Scope                                         | Normalize? | Justification            |
| --------------------------------------------- | ---------- | ------------------------ |
| Text nodes                                    | Yes        | Primary content          |
| HTML `<script>`, `<style>`, `<pre>`, `<code>` | No         | Prevents code corruption |
| Markdown code blocks/fences/inline            | No         | Preserves syntax         |
| HTML attribute values                         | No         | Ensures functionality    |

---

## RULE 9: `normalization_form` — Unicode Canonical Equivalence

| Form                      | Applied?                 | Justification                                                      |
| ------------------------- | ------------------------ | ------------------------------------------------------------------ |
| NFC (Composed)            | **Always** (first stage) | Canonical composed form — most compact, best for text comparison   |
| NFD (Decomposed)          | No (internal only)       | Used internally for diacritic operations, always recomposed to NFC |
| NFKC/NFKD (Compatibility) | Optional stage           | Lossy compatibility decomposition (e.g., ﬁ→fi, ℃→°C) — opt-in only |

**Rationale**:

- NFC is the W3C/WHATWG standard for web text
- NFC matches user expectations (é, not e + ́)
- NFC enables efficient string comparison

---

## Scope and Philosophy

Normy is **linguistically conservative** and **non-destructive by default**.

### Explicitly Out of Scope

Normy intentionally does **not** perform:

1. Titlecasing – use Unicode default or ICU
2. Full romanization – use dedicated libraries (Unidecode, AnyAscii)
3. Dictionary-based tokenization – use language-specific tools (MeCab, PyThaiNLP, etc.)
4. Lemmatization, spell correction, locale-aware sorting, smart quotes/dashes, number formatting

# 📚 NORMY LANGUAGE PROPERTY RULES: Linguistic Truth, Not Technical Convenience

> These rules define when each field should be populated, based on **LINGUISTIC properties** of the language, NOT on technical ASCII limitations or search convenience. Normy respects languages as they are.

---

## RULE 1: `case_map` — LOCALE-SPECIFIC CASE CONVERSIONS

Populate **ONLY** when:

* ✓ Language has case rules that **DIFFER** from Unicode default
* ✓ Linguistically **incorrect** to use standard `to_lowercase()`

### Case Examples

| Status | Language | Rule | Note |
| :--- | :--- | :--- | :--- |
| **✓** | Turkish | İ→i, I→ı | Dotted/dotless distinction |
| **✓** | Catalan | L·L→l·l | Geminate L with middot |
| **✗** | English | EMPTY | Unicode default is correct |
| **✗** | German | EMPTY | ß lowercase is handled by Unicode |

> **Test:** "Would a native speaker say Unicode's `to_lowercase()` is **WRONG**?"

---

## RULE 2: `fold` — SEARCH-EQUIVALENCE FOLDING (LINGUISTIC)

Populate **ONLY** when:

* ✓ Language has **OFFICIAL** search equivalence rules (one-to-many)
* ✓ Native speakers consider two forms "**the same for search**"
* ✓ The expansion is **LINGUISTIC**, not ASCII fallback

### Fold Examples

| Status | Language | Rule | Note |
| :--- | :--- | :--- | :--- |
| **✓** | German | ß→"ss", ẞ→"ss" | Official spelling rule: Straße = Strasse in search |
| **✓** | Dutch | Ĳ→"ij" | Ligature expands to digraph, IJ = ij for search |
| **✗** | French | Œ→"oe" | **NOT fold!** œ ≠ oe for search. Goes in `transliterate`. |
| **✗** | Spanish | Ñ→"n" | **NOT fold!** ñ ≠ n. This is ASCII fallback. |

> **Test:** "Do native speakers use this expansion **IN THEIR LANGUAGE**?"

---

## RULE 3: `transliterate` — HISTORICAL/OFFICIAL ASCII CONVENTIONS

Populate **ONLY** when:

* ✓ Language has **DOCUMENTED historical transliteration convention**
* ✓ Used in official contexts (postal, telegraph, international)
* ✓ Convention exists **BEFORE computers** (not invented for ASCII)

### Transliterate Examples

| Status | Language | Rule | Note |
| :--- | :--- | :--- | :--- |
| **✓** | German | Ä→"ae", Ö→"oe", Ü→"ue" | Railway timetables, Goethe historical convention |
| **✓** | Scandinavian | Å→"aa", Ä→"ae", Ö→"oe" | Postal addressing convention |
| **✓** | Icelandic | Þ→"th", Ð→"d" | International convention, e.g., Reykjavik |
| **✗** | Turkish | Ğ→"g", Ş→"s" | **NO tradition!** Pure ASCII workaround |
| **✗** | Czech/Polish | Č→"c", Ł→"l", etc. | **NO tradition!** Pure ASCII workaround |

> **Test:** "Did this convention exist in the telegraph/postal era?"

---

## RULE 4: `strip` — ACCENT-INSENSITIVE SEARCH (USER INTENT)

Populate when:

* ✓ Accents mark **PRONUNCIATION**, not distinct phonemes
* ✓ Native speakers **commonly omit accents** in informal writing
* ✓ Accent-insensitive search is **culturally expected**

### Strip Examples

| Status | Language | Rule | Note |
| :--- | :--- | :--- | :--- |
| **✓** | French | é→e, è→e | Informal writing drops accents, search expects it |
| **✓** | Spanish | á→a, ñ→n | SMS/informal commonly omits accents |
| **✓** | Vietnamese | ạ→a, ặ→a | Tone marks, but both accented and unaccented forms used |
| **✓** | Czech/Polish | ď→d, ł→l | International search use case |
| **✗** | German | Ä→A | **WRONG!** ä is distinct letter, not "a with accent" |
| **✗** | Turkish | Ğ→G | **WRONG!** ğ is distinct phoneme, not "g with accent" |

> **Test:** "Do informal writers (SMS/chat) commonly drop this mark?"
>
> **Note:** `strip` is **OPT-IN**. Users must explicitly request accent-insensitive search.

---

## RULE 5: `diac` — TRUE SPACING/COMBINING MARKS

Populate **ONLY** when:

* ✓ Marks that **NEVER** form precomposed letters in NFC (Unicode Normalization Form C)
* ✓ Optional decorative marks (harakat, nikkud, tone marks)
* ✓ Safe to strip without destroying base letters

### Diacritics Examples

| Status | Language | Mark | Note |
| :--- | :--- | :--- | :--- |
| **✓** | Arabic | harakat (tashkīl) | Optional vowel marks |
| **✓** | Hebrew | nikkud | Optional vowel points |
| **✓** | Thai | tone marks | Marks above/below consonants |
| **✗** | Czech | U+030C caron | **WRONG!** Forms precomposed letters like ď, ť, ň. |
| **✗** | French | U+0301 acute | **WRONG!** Forms precomposed letters like é. |

> **Test:** "Is this mark used standalone in real text (NFC)?"

---

## RULE 6: `segment` — SCRIPT BOUNDARY RULES

Set to **`true`** **ONLY** when:

* ✓ Language uses a script that **REQUIRES segmentation** (CJK, Thai, Lao, etc.)
* ✓ **No spaces** between words in standard writing

### `segment_rules`

* `WesternToScript`: Insert boundary when Latin → CJK/Thai/etc.
* `ScriptToWestern`: Insert boundary when CJK/Thai → Latin.
* `CJKIdeographUnigram`: Break every CJK character (Chinese only, **NOT** Japanese).

---

## RULE 7: `peek_ahead` — CONTEXT-SENSITIVE FOLDING

Set to **`true`** **ONLY** when:

* ✓ Language has **multi-character sequences** that fold as a unit
* ✓ Cannot be represented as single character fold

### Peek Ahead Example

| Status | Language | Rule | Note |
| :--- | :--- | :--- | :--- |
| **✓** | Dutch | I + J → "ij" | Two separate chars become digraph |
| **✗** | German | ß→"ss" | Single char, use **`fold`** not `peek_ahead` |

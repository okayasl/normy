use std::error::Error;

use normy::{
    COLLAPSE_WHITESPACE_UNICODE, CaseFold, DEU, ENG, FRA, JPN, LowerCase, Normy, RemoveDiacritics,
    SegmentWords, StripHtml, StripMarkdown, TUR, Transliterate, UnifyWidth, ZHO,
};

fn main() -> Result<(), Box<dyn Error>> {
    // ────────────────────────────────────────────────────────────────
    // TURKISH (Turkey) – famous for its dotted/dotless I distinction
    // ────────────────────────────────────────────────────────────────
    let tur = Normy::builder()
        .lang(TUR)
        .add_stage(LowerCase) // Critical: İ → i, I → ı
        .build();

    println!(
        "Turkish : {}",
        tur.normalize("KIZILIRMAK NEHRİ TÜRKİYE'NİN EN UZUN NEHRİDİR.")?
    );
    // → kızılırmak nehri türkiye'nin en uzun nehridir.

    // ────────────────────────────────────────────────────────────────
    // GERMAN (Germany/Austria/Switzerland) – ß and umlaut handling
    // ────────────────────────────────────────────────────────────────
    let deu = Normy::builder()
        .lang(DEU)
        .add_stage(CaseFold) // ß → ss
        .add_stage(Transliterate) // Ä → ae, Ö → oe, Ü → ue
        .build();

    println!(
        "German  : {}",
        deu.normalize("Grüße aus München! Die Straße ist sehr schön.")?
    );
    // → gruesse aus muenchen! die strasse ist sehr schoen.

    // ────────────────────────────────────────────────────────────────
    // FRENCH (France/Belgium/Canada/etc.) – classic accented text
    // ────────────────────────────────────────────────────────────────
    let fra = Normy::builder()
        .lang(FRA)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics) // é → e, ç → c, etc.
        .build();

    println!(
        "French  : {}",
        fra.normalize("Bonjour ! J'adore le café et les croissants à Paris.")?
    );
    // → bonjour ! j'adore le cafe et les croissants a paris.

    // ────────────────────────────────────────────────────────────────
    // CHINESE (Simplified – China) – fullwidth & word segmentation
    // ────────────────────────────────────────────────────────────────
    let zho = Normy::builder()
        .lang(ZHO)
        .add_stage(UnifyWidth)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .add_stage(SegmentWords) // unigram segmentation
        .build();

    println!(
        "Chinese : {}",
        zho.normalize("北京的秋天特别美丽，长城非常壮观！")?
    );
    // → 北京的秋天特别美丽 , 长城非常壮观 !

    // ────────────────────────────────────────────────────────────────
    // CHINESE (Simplified – China) – fullwidth & word segmentation & unigram cjk
    // ────────────────────────────────────────────────────────────────
    let zho = Normy::builder()
        .lang(ZHO)
        .modify_lang(|le| le.set_unigram_cjk(true))
        .add_stage(UnifyWidth)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .add_stage(SegmentWords) // unigram segmentation
        .build();

    println!(
        "Chinese(unigram cjk) : {}",
        zho.normalize("北京的秋天特别美丽，长城非常壮观！")?
    );
    // → 北 京 的 秋 天 特 别 美 丽 , 长 城 非 常 壮 观 !

    // ────────────────────────────────────────────────────────────────
    // JAPANESE (Japan) – script transitions + width unification
    // ────────────────────────────────────────────────────────────────
    let jpn = Normy::builder()
        .lang(JPN)
        .add_stage(UnifyWidth)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .add_stage(SegmentWords) // script boundary segmentation
        .build();

    println!(
        "Japanese: {}",
        jpn.normalize("東京は本当に素晴らしい街です！桜がとてもきれい。")?
    );
    // → 東京は本当に素晴らしい街です ! 桜がとてもきれい 。

    // ────────────────────────────────────────────────────────────────
    // StripHtml – Cleaning web-scraped / user-generated HTML content
    // ────────────────────────────────────────────────────────────────
    let html_cleaner = Normy::builder()
        .lang(ENG) // language usually doesn't matter here
        .add_stage(StripHtml) // removes tags, decodes entities → non-fusable
        .add_stage(LowerCase) // fusion starts from here
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .build();

    let dirty_html = r#"
        <div class="post">
            <h1>Welcome to my blog!</h1>
            <p>Today's special: café &amp; croissants ☕&nbsp;🥐</p>
            <script>alert("hacked!")</script>
        </div>
    "#;

    let cleaned = html_cleaner.normalize(dirty_html)?;
    println!("Cleaned HTML → {}", cleaned.trim());
    // → welcome to my blog! today's special: café & croissants ☕ 🥐

    // ────────────────────────────────────────────────────────────────
    // StripMarkdown – Processing GitHub issues, Discord messages, docs
    // ────────────────────────────────────────────────────────────────
    let md_cleaner = Normy::builder()
        .lang(ENG)
        .add_stage(StripMarkdown) // removes bold/italic/links/code blocks → non-fusable
        .add_stage(LowerCase)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .build();

    let github_issue = r#"**Great library!** But I found a small issue with `normalize()`..."#;

    let plain_text = md_cleaner.normalize(github_issue)?;
    println!("Cleaned Markdown → {}", plain_text.trim());
    // → great library! but i found a small issue with normalize()...

    // ────────────────────────────────────────────────────────────────
    // Typical real-world pipeline: HTML + content normalization (Turkish example)
    // ────────────────────────────────────────────────────────────────
    let web_turkish = Normy::builder()
        .lang(TUR)
        .add_stage(StripHtml) // first – non-fusable
        .add_stage(LowerCase) // İ → i, I → ı (fusion starts)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .build();

    let forum_post = r#"<p>İstanbul'un en güzel    semtleri: <strong>Beşiktaş</strong> &amp; <em>Kadıköy</em></p>"#;
    let normalized = web_turkish.normalize(forum_post)?;
    println!("Turkish web content → {}", normalized.trim());
    // → istanbul'un en güzel semtleri: beşiktaş & kadıköy

    Ok(())
}

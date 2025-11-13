#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    English,
    Turkish,
    Japanese,
    Arabic,
    Spanish,
    German,
    Unknown,
}

impl Lang {
    pub fn from_code(code: &str) -> Self {
        match code.to_lowercase().as_str() {
            "en" => Lang::English,
            "tr" => Lang::Turkish,
            "ja" => Lang::Japanese,
            "ar" => Lang::Arabic,
            "es" => Lang::Spanish,
            "de" => Lang::German,
            _ => Lang::Unknown,
        }
    }
}

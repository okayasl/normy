use crate::lang::Lang;

#[derive(Debug, Clone)]
pub struct Context {
    pub lang: Lang,
    // later: add locale data, caches, SIMD availability, etc.
}

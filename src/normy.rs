use crate::{
    Lang,
    context::Context,
    lang::DEFAULT_LANG,
    process::{ChainedProcess, DynProcess, EmptyProcess, Process},
    profile::{Profile, ProfileError},
    stage::{Stage, StageError},
};
use std::borrow::Cow;
use thiserror::Error;

use crate::stage::Utf8Validate as DefaultValidate;

#[derive(Debug, Error)]
pub enum NormyError {
    #[error(transparent)]
    Stage(#[from] StageError),
    #[error(transparent)]
    Profile(#[from] ProfileError),
}

pub struct Normy<P: Process> {
    ctx: Context,
    pipeline: P,
}

impl<P: Process> Normy<P> {
    // Public API: &str → zero-copy default
    pub fn normalize<'a>(&self, text: &'a str) -> Result<Cow<'a, str>, NormyError> {
        self.pipeline
            .process(Cow::Borrowed(text), &self.ctx)
            .map_err(Into::into)
    }

    // Profile API: accepts Cow because it may be in the middle of a pipeline
    pub fn normalize_with_profile<'a, Q: Process>(
        &self,
        profile: &Profile<Q>,
        text: &'a str,
    ) -> Result<Cow<'a, str>, NormyError> {
        profile
            .run(Cow::Borrowed(text), &self.ctx)
            .map_err(Into::into)
    }
}

pub struct NormyBuilder<P: Process> {
    lang: Lang,
    current: P,
}

impl Default for NormyBuilder<EmptyProcess> {
    fn default() -> Self {
        Self {
            lang: DEFAULT_LANG,
            current: EmptyProcess,
        }
    }
}

impl<P: Process> NormyBuilder<P> {
    pub fn lang(mut self, lang: Lang) -> Self {
        self.lang = lang;
        self
    }

    pub fn add_stage<S: Stage + 'static>(self, stage: S) -> NormyBuilder<ChainedProcess<S, P>> {
        NormyBuilder {
            lang: self.lang,
            current: ChainedProcess {
                stage,
                previous: self.current,
            },
        }
    }

    /// **Optional** Must be called **first** in production.
    #[inline(always)]
    pub fn with_validation(self) -> NormyBuilder<ChainedProcess<DefaultValidate, P>> {
        self.add_stage(DefaultValidate)
    }

    pub fn build(self) -> Normy<P> {
        let ctx = Context { lang: self.lang };
        Normy {
            ctx,
            pipeline: self.current,
        }
    }
}

impl Normy<EmptyProcess> {
    pub fn builder() -> NormyBuilder<EmptyProcess> {
        NormyBuilder::default()
    }
}

impl Normy<DynProcess> {
    /// Start a **plugin** pipeline – stages are added at run-time.
    pub fn plugin_builder() -> DynNormyBuilder {
        DynNormyBuilder::new()
    }
}

/* ---------- Dyn builder ---------- */
pub struct DynNormyBuilder {
    lang: Lang,
    pipeline: DynProcess,
}

impl DynNormyBuilder {
    fn new() -> Self {
        Self {
            lang: DEFAULT_LANG,
            pipeline: DynProcess::new(),
        }
    }

    pub fn lang(mut self, lang: Lang) -> Self {
        self.lang = lang;
        self
    }

    pub fn add_stage<T: Stage + Send + Sync + 'static>(self, stage: T) -> Self {
        Self {
            pipeline: self.pipeline.push(stage),
            ..self
        }
    }

    /// **Optional** Must be called **first** in production.
    #[inline(always)]
    pub fn with_validation(self) -> Self {
        self.add_stage(DefaultValidate)
    }

    pub fn build(self) -> Normy<DynProcess> {
        let ctx = Context { lang: self.lang };
        Normy {
            ctx,
            pipeline: self.pipeline,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Normy,
        lang::TUR,
        profile::Profile,
        stage::{lower_case::Lowercase, trim_whitespace::TrimWhitespace},
    };

    #[test]
    fn test_simple_normy() {
        let normy = Normy::builder()
            .with_validation()
            .lang(TUR)
            .add_stage(Lowercase)
            .add_stage(TrimWhitespace)
            .build();
        let result = normy.normalize("  İSTANBUL ").unwrap();
        assert_eq!(result.to_string(), "istanbul")
    }

    #[test]
    fn test_simple_normy_default() {
        let normy = Normy::builder()
            .with_validation()
            .add_stage(Lowercase)
            .add_stage(TrimWhitespace)
            .build();
        let result = normy.normalize("  IŞiK ").unwrap();
        assert_eq!(result.to_string(), "işik")
    }

    #[test]
    fn test_simple_plugin_normy() {
        let normy = Normy::plugin_builder()
            .lang(TUR)
            .add_stage(Lowercase)
            .add_stage(TrimWhitespace)
            .build();
        let result = normy.normalize(" İSTANBUL ").unwrap();
        assert_eq!(result.to_string(), "istanbul")
    }

    #[test]
    fn test_simple_normy_with_profile() {
        let normy = Normy::builder().lang(TUR).build();
        let profile = Profile::builder("test")
            .add_stage(Lowercase)
            .add_stage(TrimWhitespace)
            .build();
        let result = normy
            .normalize_with_profile(&profile, "  İSTANBUL ")
            .unwrap();
        assert_eq!(result.to_string(), "istanbul")
    }

    #[test]
    fn test_simple_normy_with_dynprofile() {
        let normy = Normy::builder().with_validation().lang(TUR).build();
        let profile = Profile::plugin_builder("test")
            .add_stage(Lowercase)
            .add_stage(TrimWhitespace)
            .build();
        let result = normy
            .normalize_with_profile(&profile, "  İSTANBUL ")
            .unwrap();
        assert_eq!(result.to_string(), "istanbul")
    }
}

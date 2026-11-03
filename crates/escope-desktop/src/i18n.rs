//! Desktop-owned copy lives in Fluent catalogs. Device data never enters this layer.
use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

pub const STORAGE_KEY: &str = "embedscope.locale";
const FALLBACK: usize = 0;

pub struct Locale {
    pub id: &'static str,
    pub name: &'static str,
    pub docs_prefix: &'static str,
    pub source: &'static str,
}

// Keep English first: it is the final fallback. Preferences persist IDs, not indices.
// Adding a locale requires its catalog and one entry here; views stay language-neutral.
pub const LOCALES: &[Locale] = &[
    Locale {
        id: "en-US",
        name: "English",
        docs_prefix: "en/",
        source: include_str!("../locales/en-US.ftl"),
    },
    Locale {
        id: "zh-CN",
        name: "简体中文",
        docs_prefix: "",
        source: include_str!("../locales/zh-CN.ftl"),
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preference {
    System,
    Language(usize),
}

impl Preference {
    pub fn from_stored(value: Option<&str>) -> Self {
        value
            .and_then(|id| LOCALES.iter().position(|locale| locale.id == id))
            .map_or(Self::System, Self::Language)
    }

    pub fn stored(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Language(index) => LOCALES[index].id,
        }
    }
}

pub struct I18n {
    bundles: Vec<FluentBundle<FluentResource>>,
    preference: Preference,
    system: usize,
}

impl I18n {
    pub fn new(stored: Option<&str>, system_languages: impl IntoIterator<Item = String>) -> Self {
        Self {
            bundles: LOCALES.iter().map(bundle).collect(),
            preference: Preference::from_stored(stored),
            system: match_system(system_languages),
        }
    }

    pub fn preference(&self) -> Preference {
        self.preference
    }

    pub fn set_preference(&mut self, preference: Preference) {
        self.preference = preference;
    }

    fn active(&self) -> usize {
        match self.preference {
            Preference::System => self.system,
            Preference::Language(index) => index,
        }
    }

    pub fn locale(&self) -> &'static Locale {
        &LOCALES[self.active()]
    }

    pub fn docs_url(&self, page: &str) -> String {
        format!(
            "https://umbrella22.github.io/EmbedScope_website/{}guide/basic/{page}.html",
            self.locale().docs_prefix
        )
    }

    pub fn text(&self, id: &str) -> String {
        self.format(id, None)
    }

    pub fn format(&self, id: &str, args: Option<&FluentArgs<'_>>) -> String {
        for index in [self.active(), FALLBACK] {
            if let Some(value) = format(&self.bundles[index], id, args) {
                return value;
            }
        }
        // Keep an actionable message ID if even the English catalog is incomplete.
        id.to_owned()
    }
}

fn match_system(languages: impl IntoIterator<Item = String>) -> usize {
    let supported: Vec<LanguageIdentifier> = LOCALES
        .iter()
        .map(|locale| locale.id.parse().expect("valid embedded locale ID"))
        .collect();
    for language in languages {
        let Ok(requested) = language.parse::<LanguageIdentifier>() else {
            continue;
        };
        if let Some(index) = supported.iter().position(|id| *id == requested) {
            return index;
        }
        if let Some(index) = supported
            .iter()
            .position(|id| id.language == requested.language)
        {
            return index;
        }
    }
    FALLBACK
}

fn bundle(locale: &Locale) -> FluentBundle<FluentResource> {
    let mut bundle = FluentBundle::new(vec![locale.id.parse().expect("valid embedded locale ID")]);
    // Both supported languages use left-to-right text; preserve literal paths and IDs.
    bundle.set_use_isolating(false);
    let resource = FluentResource::try_new(locale.source.to_owned())
        .unwrap_or_else(|(resource, _errors)| resource);
    // Recover usable messages from a malformed catalog; tests reject catalog errors.
    let _ = bundle.add_resource(resource);
    bundle
}

fn format(
    bundle: &FluentBundle<FluentResource>,
    id: &str,
    args: Option<&FluentArgs<'_>>,
) -> Option<String> {
    let message = bundle.get_message(id)?;
    let mut errors = Vec::new();
    let value = bundle.format_pattern(message.value()?, args, &mut errors);
    errors.is_empty().then(|| value.into_owned())
}

// Literal keys are easy to search and validate. Named, typed arguments support Fluent plurals.
macro_rules! tr {
    ($i18n:expr, $id:literal) => { $i18n.text($id) };
    ($i18n:expr, $id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = fluent_bundle::FluentArgs::new();
        $(args.set(stringify!($name), $value);)+
        $i18n.format($id, Some(&args))
    }};
}
pub(crate) use tr;

#[cfg(test)]
mod tests;

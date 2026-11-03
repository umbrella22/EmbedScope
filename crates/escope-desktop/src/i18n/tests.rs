use super::*;
use fluent_syntax::ast::{
    CallArguments, Entry, Expression, InlineExpression, Pattern, PatternElement,
};
use std::collections::{BTreeMap, BTreeSet};

type Signature = BTreeMap<String, BTreeSet<String>>;

fn variables(pattern: &Pattern<&str>, names: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let PatternElement::Placeable { expression: expr } = element {
            expression(expr, names);
        }
    }
}

fn expression(expr: &Expression<&str>, names: &mut BTreeSet<String>) {
    match expr {
        Expression::Inline(value) => inline(value, names),
        Expression::Select { selector, variants } => {
            inline(selector, names);
            for variant in variants {
                variables(&variant.value, names);
            }
        }
    }
}

fn arguments(args: &CallArguments<&str>, names: &mut BTreeSet<String>) {
    for value in &args.positional {
        inline(value, names);
    }
    for value in &args.named {
        inline(&value.value, names);
    }
}

fn inline(value: &InlineExpression<&str>, names: &mut BTreeSet<String>) {
    match value {
        InlineExpression::VariableReference { id } => {
            names.insert(id.name.to_owned());
        }
        InlineExpression::Placeable { expression: expr } => expression(expr, names),
        InlineExpression::FunctionReference {
            arguments: args, ..
        } => arguments(args, names),
        InlineExpression::TermReference {
            arguments: Some(args),
            ..
        } => arguments(args, names),
        _ => (),
    }
}

fn signatures(locale: &Locale) -> Signature {
    let resource = FluentResource::try_new(locale.source.to_owned()).expect(locale.id);
    let mut bundle = FluentBundle::new(vec![locale.id.parse().unwrap()]);
    bundle.add_resource(resource).expect("no duplicate IDs");
    let ast = fluent_syntax::parser::parse(locale.source).expect(locale.id);
    let mut result = Signature::new();
    for entry in ast.body {
        let (id, value, attributes) = match entry {
            Entry::Message(message) => (
                message.id.name.to_owned(),
                message.value,
                message.attributes,
            ),
            Entry::Term(term) => (
                format!("-{}", term.id.name),
                Some(term.value),
                term.attributes,
            ),
            _ => continue,
        };
        let mut params = BTreeSet::new();
        if let Some(pattern) = value {
            variables(&pattern, &mut params);
        }
        assert!(
            result.insert(id.clone(), params).is_none(),
            "duplicate {id}"
        );
        for attribute in attributes {
            let mut params = BTreeSet::new();
            variables(&attribute.value, &mut params);
            result.insert(format!("{id}.{}", attribute.id.name), params);
        }
    }
    result
}

#[test]
fn catalogs_have_equal_keys_arguments_and_all_ui_references_resolve() {
    let expected = signatures(&LOCALES[FALLBACK]);
    let mut ids = BTreeSet::new();
    for locale in LOCALES {
        assert!(ids.insert(locale.id), "duplicate locale ID");
        assert_eq!(
            signatures(locale),
            expected,
            "catalog mismatch: {}",
            locale.id
        );
        let bundle = bundle(locale);
        // Exercise every pattern with its named parameters, beyond parsing alone.
        for (id, parameters) in &expected {
            // Terms and attributes resolve through referencing messages.
            if id.starts_with('-') || id.contains('.') {
                continue;
            }
            let mut args = FluentArgs::new();
            for name in parameters {
                args.set(name.as_str(), 2);
            }
            assert!(
                format(&bundle, id, Some(&args)).is_some(),
                "{}: {id}",
                locale.id
            );
        }
    }
    for source in [
        include_str!("../app.rs"),
        include_str!("../app/chrome.rs"),
        include_str!("../app/inspector.rs"),
        include_str!("../app/diagnostics.rs"),
    ] {
        for call in source.split("tr!(").skip(1) {
            let id = call.split('"').nth(1).expect("literal message ID");
            assert!(expected.contains_key(id), "missing UI message {id}");
        }
    }
}

#[test]
fn preferred_system_languages_match_and_choices_survive_storage() {
    for (languages, expected) in [
        (vec!["zh-CN"], "zh-CN"),
        (vec!["en-GB"], "en-US"),
        (vec!["zh-Hans-SG"], "zh-CN"),
        (vec!["ja-JP", "zh-CN", "en-US"], "zh-CN"),
        (vec!["invalid_locale", "fr-FR"], "en-US"),
        (vec![], "en-US"),
    ] {
        let i18n = I18n::new(None, languages.into_iter().map(str::to_owned));
        assert_eq!(i18n.locale().id, expected);
    }
    let mut i18n = I18n::new(Some("en-US"), ["zh-CN".into()]);
    assert_eq!(i18n.locale().id, "en-US", "explicit choice overrides OS");
    for preference in
        std::iter::once(Preference::System).chain((0..LOCALES.len()).map(Preference::Language))
    {
        i18n.set_preference(preference);
        let restored = I18n::new(Some(preference.stored()), ["zh-CN".into()]);
        assert_eq!(restored.preference(), preference);
        assert_eq!(restored.locale().id, i18n.locale().id);
    }
    assert_eq!(
        Preference::from_stored(Some("removed-locale")),
        Preference::System
    );
}

#[test]
fn broken_or_missing_translations_fall_back_and_raw_values_stay_literal() {
    let mut i18n = I18n::new(Some("zh-CN"), []);
    let error = "/tmp/设置/原样.png: permission denied";
    let message = tr!(i18n, "error-context", error = error);
    assert!(message.starts_with("操作失败："));
    assert!(message.ends_with(error));
    assert!(i18n.docs_url("inspector").contains("/guide/basic/"));
    i18n.bundles[1] = bundle(&Locale {
        source: "refresh = { $unprovided }\n",
        ..LOCALES[1]
    });
    assert_eq!(
        tr!(i18n, "refresh"),
        "Refresh",
        "invalid pattern falls back"
    );
    assert_eq!(
        tr!(i18n, "connect"),
        "Connect",
        "missing message falls back"
    );
    assert_eq!(i18n.text("unknown-key"), "unknown-key");
    i18n.set_preference(Preference::Language(0));
    assert!(i18n.docs_url("inspector").contains("/en/guide/basic/"));
}

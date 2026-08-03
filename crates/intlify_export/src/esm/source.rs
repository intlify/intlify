// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Canonical bounded source writers for built-in ESM artifacts.
//!
//! These writers own the observable UTF-8, whitespace, declaration ordering,
//! and scalar escaping of format `0.1`. They consume only checked semantic
//! values and never invoke a formatter, serializer, runtime, or filesystem.

use intlify_contract::DeliveryUnitSegment;
use intlify_contract::{CatalogKey, Locale};
use intlify_linker::MessageBundlePlan;

use super::{EsmExporterOptions, LocaleAsset};
use crate::writer::{BoundedExportWriter, ExportPayloadBudget};
use crate::{ExportArtifactPayload, ExportError, MessageArgumentSignature, ValidatedTypedOutput};

pub(super) fn render_locale_module(
    budget: &mut ExportPayloadBudget,
    plan: &MessageBundlePlan,
) -> Result<ExportArtifactPayload, ExportError> {
    let mut writer = budget.writer();
    writer.try_write_str("export const formatVersion = [0, 1];\n")?;
    writer.try_write_str("export const deliveryUnit = [")?;
    write_string_values(
        &mut writer,
        plan.delivery_unit()
            .segments()
            .iter()
            .map(DeliveryUnitSegment::as_str),
    )?;
    writer.try_write_str("];\nexport const locale = ")?;
    write_js_string(&mut writer, plan.locale().as_str())?;
    writer.try_write_str(";\n\n")?;

    if plan.messages().is_empty() {
        writer.try_write_str("export const messages = [];\n")?;
        return writer.finish();
    }

    writer.try_write_str("export const messages = [\n")?;
    for (index, message) in plan.messages().iter().enumerate() {
        let scope = message.resolved_scope().as_catalog_scope();
        writer.try_write_str("  [[")?;
        write_js_string(&mut writer, scope.namespace().as_str())?;
        writer.try_write_str(", ")?;
        write_js_string(&mut writer, scope.name().as_str())?;
        writer.try_write_str("], ")?;
        write_js_string(&mut writer, message.domain().as_str())?;
        writer.try_write_str(", ")?;
        write_js_string(&mut writer, message.key().as_str())?;
        writer.try_write_str(", ")?;
        write_js_string(&mut writer, message.definition_locale().as_str())?;
        writer.try_write_str(", ")?;
        write_js_string(&mut writer, message.message().as_str())?;
        writer.try_write_str(if index + 1 == plan.messages().len() {
            "]\n"
        } else {
            "],\n"
        })?;
    }
    writer.try_write_str("];\n")?;
    writer.finish()
}

pub(super) fn render_loader_module(
    budget: &mut ExportPayloadBudget,
    options: &EsmExporterOptions,
    assets: &[LocaleAsset<'_>],
) -> Result<ExportArtifactPayload, ExportError> {
    let mut writer = budget.writer();
    let mut eager_count = 0usize;
    for asset in assets {
        let Some(alias) = asset.eager_alias else {
            continue;
        };
        if alias != eager_count {
            return Err(super::artifact_assembly_error());
        }
        writer.try_write_str("import * as eager")?;
        writer.try_write_str(&alias.to_string())?;
        writer.try_write_str(" from ")?;
        write_js_string(
            &mut writer,
            &super::path::relative_module_specifier(&asset.path),
        )?;
        writer.try_write_str(";\n")?;
        eager_count += 1;
    }
    if eager_count != 0 {
        writer.try_write_str("\n")?;
    }

    writer.try_write_str("export const formatVersion = [0, 1];\n")?;
    writer.try_write_str("export const locales = [")?;
    write_string_values(
        &mut writer,
        options.production_locales().iter().map(Locale::as_str),
    )?;
    writer.try_write_str("];\n")?;
    write_fallbacks(&mut writer, options)?;
    writer.try_write_str("\nexport function loadLocale(locale) {\n  switch (locale) {\n")?;
    for asset in assets {
        writer.try_write_str("    case ")?;
        write_js_string(&mut writer, asset.plan.locale().as_str())?;
        writer.try_write_str(":\n      return ")?;
        if let Some(alias) = asset.eager_alias {
            writer.try_write_str("Promise.resolve(eager")?;
            writer.try_write_str(&alias.to_string())?;
            writer.try_write_str(")")?;
        } else {
            writer.try_write_str("import(")?;
            write_js_string(
                &mut writer,
                &super::path::relative_module_specifier(&asset.path),
            )?;
            writer.try_write_str(")")?;
        }
        writer.try_write_str(";\n")?;
    }
    writer.try_write_str(
        "    default:\n      return Promise.reject(new RangeError(\"unsupported locale\"));\n  }\n}\n",
    )?;
    writer.finish()
}

pub(super) fn render_accessor_module(
    budget: &mut ExportPayloadBudget,
    output: &ValidatedTypedOutput<'_>,
) -> Result<ExportArtifactPayload, ExportError> {
    let mut writer = budget.writer();
    let scope = output.model().resolved_scope().as_catalog_scope();
    writer.try_write_str("export const formatVersion = [0, 1] as const;\n")?;
    writer.try_write_str("export const scope = [")?;
    write_js_string(&mut writer, scope.namespace().as_str())?;
    writer.try_write_str(", ")?;
    write_js_string(&mut writer, scope.name().as_str())?;
    writer.try_write_str("] as const;\n\n")?;

    write_message_key_type(&mut writer, output)?;
    writer.try_write_str("\n")?;
    write_message_arguments_type(&mut writer, output)?;
    writer.try_write_str("\n")?;
    write_message_call_type(&mut writer, output)?;
    writer.try_write_str(
        "\nexport interface MessageRuntime<Result> {\n  format(messageScope: typeof scope, ...call: MessageCall): Result;\n}\n\nexport function createMessageAccessor<Result>(runtime: MessageRuntime<Result>) {\n  return function t(...call: MessageCall): Result {\n    return runtime.format(scope, ...call);\n  };\n}\n",
    )?;
    writer.finish()
}

fn write_message_key_type(
    writer: &mut BoundedExportWriter<'_>,
    output: &ValidatedTypedOutput<'_>,
) -> Result<(), ExportError> {
    if output.messages().is_empty() {
        return writer.try_write_str("export type MessageKey = never;\n");
    }
    writer.try_write_str("export type MessageKey =\n")?;
    for (index, message) in output.messages().iter().enumerate() {
        writer.try_write_str("  | ")?;
        write_readonly_key_tuple(writer, message.key())?;
        writer.try_write_str(if index + 1 == output.messages().len() {
            ";\n"
        } else {
            "\n"
        })?;
    }
    Ok(())
}

fn write_message_arguments_type(
    writer: &mut BoundedExportWriter<'_>,
    output: &ValidatedTypedOutput<'_>,
) -> Result<(), ExportError> {
    if output.messages().is_empty() {
        return writer
            .try_write_str("export type MessageArguments<K extends MessageKey> = never;\n");
    }

    writer.try_write_str("export type MessageArguments<K extends MessageKey> = K extends ")?;
    for (index, message) in output.messages().iter().enumerate() {
        if index != 0 {
            write_indent(writer, index)?;
            writer.try_write_str(": K extends ")?;
        }
        write_readonly_key_tuple(writer, message.key())?;
        writer.try_write_str("\n")?;
        write_indent(writer, index + 1)?;
        writer.try_write_str("? ")?;
        write_argument_object(writer, message.arguments(), index + 1)?;
        writer.try_write_str("\n")?;
    }
    write_indent(writer, output.messages().len())?;
    writer.try_write_str(": never;\n")
}

fn write_argument_object(
    writer: &mut BoundedExportWriter<'_>,
    arguments: &[MessageArgumentSignature],
    question_indent: usize,
) -> Result<(), ExportError> {
    if arguments.is_empty() {
        return writer.try_write_str("Readonly<Record<string, never>>");
    }

    writer.try_write_str("Readonly<{\n")?;
    for argument in arguments {
        write_indent(writer, question_indent + 2)?;
        write_js_string(writer, argument.name())?;
        writer.try_write_str(": unknown;\n")?;
    }
    write_indent(writer, question_indent + 1)?;
    writer.try_write_str("}>")
}

fn write_message_call_type(
    writer: &mut BoundedExportWriter<'_>,
    output: &ValidatedTypedOutput<'_>,
) -> Result<(), ExportError> {
    if output.messages().is_empty() {
        return writer.try_write_str("type MessageCall = [key: never, args: never];\n");
    }

    writer.try_write_str("type MessageCall =\n")?;
    for (index, message) in output.messages().iter().enumerate() {
        writer.try_write_str("  | [key: ")?;
        write_readonly_key_tuple(writer, message.key())?;
        writer.try_write_str(", args: MessageArguments<")?;
        write_readonly_key_tuple(writer, message.key())?;
        writer.try_write_str(if index + 1 == output.messages().len() {
            ">];\n"
        } else {
            ">]\n"
        })?;
    }
    Ok(())
}

fn write_fallbacks(
    writer: &mut BoundedExportWriter<'_>,
    options: &EsmExporterOptions,
) -> Result<(), ExportError> {
    if options.fallbacks().is_empty() {
        return writer.try_write_str("export const fallbacks = [];\n");
    }

    writer.try_write_str("export const fallbacks = [\n")?;
    for (index, fallback) in options.fallbacks().iter().enumerate() {
        writer.try_write_str("  [")?;
        write_js_string(writer, fallback.source().as_str())?;
        writer.try_write_str(", [")?;
        write_string_values(writer, fallback.targets().iter().map(Locale::as_str))?;
        writer.try_write_str(if index + 1 == options.fallbacks().len() {
            "]]\n"
        } else {
            "]],\n"
        })?;
    }
    writer.try_write_str("];\n")
}

fn write_readonly_key_tuple(
    writer: &mut BoundedExportWriter<'_>,
    key: &CatalogKey,
) -> Result<(), ExportError> {
    writer.try_write_str("readonly [")?;
    write_js_string(writer, key.domain().as_str())?;
    writer.try_write_str(", ")?;
    write_js_string(writer, key.as_str())?;
    writer.try_write_str("]")
}

fn write_string_values<'a>(
    writer: &mut BoundedExportWriter<'_>,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<(), ExportError> {
    for (index, value) in values.into_iter().enumerate() {
        if index != 0 {
            writer.try_write_str(", ")?;
        }
        write_js_string(writer, value)?;
    }
    Ok(())
}

fn write_indent(writer: &mut BoundedExportWriter<'_>, levels: usize) -> Result<(), ExportError> {
    for _ in 0..levels {
        writer.try_write_str("  ")?;
    }
    Ok(())
}

fn write_js_string(writer: &mut BoundedExportWriter<'_>, value: &str) -> Result<(), ExportError> {
    writer.try_write_str("\"")?;
    for character in value.chars() {
        match character {
            '"' => writer.try_write_str("\\\"")?,
            '\\' => writer.try_write_str("\\\\")?,
            '\u{0008}' => writer.try_write_str("\\b")?,
            '\t' => writer.try_write_str("\\t")?,
            '\n' => writer.try_write_str("\\n")?,
            '\u{000c}' => writer.try_write_str("\\f")?,
            '\r' => writer.try_write_str("\\r")?,
            '\u{2028}' => writer.try_write_str("\\u2028")?,
            '\u{2029}' => writer.try_write_str("\\u2029")?,
            character if character <= '\u{001f}' => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let value = character as usize;
                writer.try_extend_from_slice(&[
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    HEX[value >> 4],
                    HEX[value & 0x0f],
                ])?;
            }
            character => {
                let mut encoded = [0; 4];
                writer.try_write_str(character.encode_utf8(&mut encoded))?;
            }
        }
    }
    writer.try_write_str("\"")
}

#[cfg(test)]
mod tests {
    use super::write_js_string;
    use crate::writer::ExportPayloadBudget;

    #[test]
    fn scalar_writer_uses_the_exact_shared_escape_contract() {
        let mut budget = ExportPayloadBudget::new();
        let mut writer = budget.writer();
        write_js_string(
            &mut writer,
            "\"\\\u{0008}\t\n\u{000c}\r\u{0000}\u{001f}\u{2028}\u{2029}é<&/'",
        )
        .unwrap();
        let payload = writer.finish().unwrap();
        assert_eq!(
            std::str::from_utf8(payload.as_bytes()).unwrap(),
            "\"\\\"\\\\\\b\\t\\n\\f\\r\\u0000\\u001f\\u2028\\u2029é<&/'\""
        );
    }
}

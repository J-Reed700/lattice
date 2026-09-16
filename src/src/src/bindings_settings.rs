//! Specta rc.20 treats serde input defaults as optional output fields. Settings
//! commands serialize complete DTOs. Derive their output presence from Serde,
//! keeping the input defaults and one Rust schema.
use lattice::application::contracts::settings::{CustomToolSettingsDto, SettingsDto};
use serde_json::Value;
use specta::{
    datatype::{DataType, StructFields},
    NamedType, TypeMap,
};
use std::collections::{BTreeMap, BTreeSet};

fn collect_fields(
    ty: &DataType,
    value: &Value,
    types: &TypeMap,
    fields: &mut BTreeMap<String, BTreeSet<String>>,
) {
    match ty {
        DataType::Reference(reference) => {
            if let Some(named) = types.get(reference.sid()) {
                collect_fields(&named.inner, value, types, fields);
            }
        }
        DataType::Struct(structure) => {
            if let (StructFields::Named(named), Some(object)) =
                (structure.fields(), value.as_object())
            {
                fields.insert(
                    structure.name().to_string(),
                    object.keys().cloned().collect(),
                );
                for (name, field) in named.fields() {
                    if let (Some(ty), Some(value)) = (field.ty(), object.get(name.as_ref())) {
                        collect_fields(ty, value, types, fields);
                    }
                }
            }
        }
        DataType::Nullable(inner) => collect_fields(inner, value, types, fields),
        DataType::List(list) => {
            if let Some(values) = value.as_array() {
                for value in values {
                    collect_fields(list.ty(), value, types, fields);
                }
            }
        }
        _ => {}
    }
}

pub fn normalize_settings_output(generated: &str) -> serde_json::Result<String> {
    let mut settings = SettingsDto::default();
    // Exercise the settings element type even though the default list is empty.
    settings
        .llm
        .custom_tools
        .push(serde_json::from_value::<CustomToolSettingsDto>(
            serde_json::json!({"name":"example", "description":"example", "endpoint":"https://example.invalid"}),
        )?);
    let mut types = TypeMap::default();
    let root = SettingsDto::definition_named_data_type(&mut types);
    let mut fields = BTreeMap::new();
    collect_fields(
        &root.inner,
        &serde_json::to_value(settings)?,
        &types,
        &mut fields,
    );
    let mut current = None;
    Ok(generated
        .lines()
        .map(|line| {
            if let Some(declaration) = line.strip_prefix("export type ") {
                current = declaration
                    .split_whitespace()
                    .next()
                    .and_then(|name| fields.get(name));
            }
            if let Some(fields) = current.filter(|_| !line.trim_start().starts_with('*')) {
                return fields.iter().fold(line.to_owned(), |output, name| {
                    output.replace(&format!("{name}?:"), &format!("{name}:"))
                });
            }
            line.to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrects_only_fields_present_in_serialized_settings() -> serde_json::Result<()> {
        let input = "export type SettingsDto = {\nprivacy?: PrivacySettingsDto;\n}\nexport type LLMSettingsDto = {\nllamaCpp?: LlamaCppSettingsDto;\n}\nexport type CustomToolSettingsDto = {\nqueryParam?: string;\n}\nexport type Other = {\noptional?: string;\n}";
        let output = normalize_settings_output(input)?;
        assert!(output.contains("privacy: PrivacySettingsDto"));
        assert!(output.contains("llamaCpp: LlamaCppSettingsDto"));
        assert!(output.contains("queryParam: string"));
        assert!(output.contains("optional?: string"));
        assert_eq!(
            normalize_settings_output("export type RetrievalTuningSettingsDto = { deepResearchDepth?: number; deepResearchBranchQueries?: number }")?,
            "export type RetrievalTuningSettingsDto = { deepResearchDepth: number; deepResearchBranchQueries: number }"
        );
        Ok(())
    }
}

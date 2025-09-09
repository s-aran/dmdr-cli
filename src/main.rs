mod command;
mod dot;

use ahash::AHashSet;
use clap::{Parser, Subcommand};
use dialoguer::Input;
use dmdr_core::model::{MetaData, MyField, MyModel, RelationType};
use std::io::{BufWriter, Write, stdout};
use std::sync::Arc;

use dmdr_core::{
    load_json,
    model::{Structure, UuidIndexes},
};

use crate::command::{Command, CommandEnumerate, CommandGet, CommandInteractive};
use crate::dot::write_dot;

pub enum FieldType {
    Local,
    Related,
    Forwarded,
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None, subcommand_required = true, arg_required_else_help = true)]
struct Args {
    #[clap(subcommand)]
    command: Commands,
    #[clap(value_name = "FILE")]
    file: String,
}

#[derive(Subcommand)]
enum Commands {
    Enumerate {
        #[clap(long = "uuid")]
        uuid: bool,
        #[clap(value_name = "MODEL")]
        model: Option<String>,
        #[clap(long = "fields")]
        fields: bool,
    },
    Write {
        #[clap(value_name = "MODEL")]
        model: Option<String>,
    },
    Get {
        #[clap(value_name = "MODEL")]
        model: String,
        #[clap(long = "fields")]
        show_fields: bool,
        #[clap(long = "meta")]
        show_meta: bool,
        #[clap(long = "source")]
        show_source: bool,
    },
    Interactive,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let (data, indexes) = load_json(args.file.into())?;

    match args.command {
        Commands::Enumerate {
            uuid,
            model,
            fields,
        } => {
            let mut instance = CommandEnumerate::new(uuid, model, fields);
            instance.command(&data, &indexes);
        }
        Commands::Write { model } => {
            let (data, indexes) = if let Some(model) = model {
                if let Some(model) = get_model_by(&indexes, model.as_str()) {
                    let uuid = model._meta_data.uuid.clone();
                    rebuild(data, indexes, uuid)
                } else {
                    panic!("no match {} in models", model);
                }
            } else {
                (data, indexes)
            };

            write_dot(&data, &indexes, Some("data.dot".into()))?;
        }
        Commands::Get {
            model,
            show_fields,
            show_meta,
            show_source,
        } => {
            let mut instance = CommandGet::new(model, show_fields, show_meta, show_source);
            instance.command(&data, &indexes);
        }
        Commands::Interactive => {
            let mut out = BufWriter::new(stdout().lock());
            write(&mut out, "> ".as_bytes());

            let mut instance = CommandInteractive::new();
            instance.command(&data, &indexes);
            while instance.continuous {
                write(&mut out, "> ".as_bytes());
            }
        }
    }

    Ok(())

    // and ...
    // > dot -Kdot -Gdpi=300 -Tpng data.dot -odata.png
}

pub fn write<T>(to: &mut BufWriter<T>, data: &[u8])
where
    T: Sized + Write,
{
    let _ = to.write_all(data);
    let _ = to.flush();
}

fn get_model_by(indexes: &UuidIndexes, model_name_or_uuid: &str) -> Option<Arc<MyModel>> {
    let specified_uuid = indexes.has_model(model_name_or_uuid);
    let specified_name = indexes.has_model_name(model_name_or_uuid);

    if specified_name {
        let by_name = indexes.get_model_by_name(model_name_or_uuid);
        let by_table = indexes.get_model_by_table(model_name_or_uuid);

        if let Some(n) = by_name {
            Some(n)
        } else if let Some(t) = by_table {
            Some(t)
        } else {
            panic!("not found: {}", model_name_or_uuid);
        }
    } else if specified_uuid {
        Some(indexes.get_model_by_uuid(model_name_or_uuid).unwrap())
    } else {
        None
    }
}
pub fn get_display_models(model: &MyModel) -> Vec<String> {
    let mut lines = vec![];

    lines.push(format!("model name: {}", model.model_name));
    lines.push(format!("object name: {}", model.object_name));
    lines.push(format!("app label: {}", model.app_label));
    lines.push(format!("db table: {}", model.db_table));
    lines.push(format!(
        "fields: local={}, related={}, forwarded={}",
        model.local_fields.len(),
        model.relation_fields.len(),
        model.forward_fields.len(),
    ));

    lines
}

pub fn get_display_fields(
    indexes: &UuidIndexes,
    field: &MyField,
    category: FieldType,
) -> Vec<String> {
    let prefix = match category {
        FieldType::Local => "Local",
        FieldType::Related => "Related",
        FieldType::Forwarded => "Forwarded",
    };

    let name = &field.name;
    let text = match category {
        FieldType::Local => format!("{name}"),
        FieldType::Related => format!("{name}"),
        FieldType::Forwarded => {
            let uuid = &field.related_model.clone().unwrap().uuid;
            let model = indexes.get_model_by_uuid(uuid).unwrap();
            format!("{name} -> {} ({})", model.object_name, model.db_table)
        }
    };

    let lines = vec![format!("[{:9}] {}", prefix, text)];

    lines
}

pub fn get_display_meta_data(meta_data: &MetaData) -> Vec<String> {
    let mut lines = vec![];

    lines.push(format!("uuid: {}", meta_data.uuid));
    lines.push(format!(
        "source file: {}:{}",
        meta_data.code.source_file, meta_data.code.line_number
    ));
    lines.push("".to_owned());

    lines
}

fn rebuild(
    data: Arc<Structure>,
    indexes: UuidIndexes,
    model_uuid: String,
) -> (Arc<Structure>, UuidIndexes) {
    // 1ホップの近傍をすべて含める
    let mut keep_models: AHashSet<String> = AHashSet::new();
    keep_models.insert(model_uuid.clone());

    let new_relations: Vec<_> = data
        .relations
        .iter()
        .filter(|rel| {
            rel.src_model_uuid == model_uuid
                || rel.target_model_uuid == model_uuid
                || rel.through_model_uuid.as_deref() == Some(model_uuid.as_str())
        })
        .cloned()
        .collect();

    for rel in &new_relations {
        keep_models.insert(rel.src_model_uuid.clone());
        keep_models.insert(rel.target_model_uuid.clone());
        if let Some(t) = &rel.through_model_uuid {
            keep_models.insert(t.clone());
        }
    }

    let new_models: Vec<_> = data
        .models
        .iter()
        .filter(|m| keep_models.contains(&m._meta_data.uuid))
        .cloned()
        .collect();

    let new_data = Structure {
        models: new_models,
        relations: new_relations,
    };

    let shared = Arc::new(new_data);
    let new_indexes = UuidIndexes::new(&shared);
    (shared, new_indexes)
}

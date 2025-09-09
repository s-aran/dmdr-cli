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

use crate::dot::write_dot;

enum FieldType {
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
        #[clap(value_name = "FIELD")]
        field: Option<String>,
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
            let instance = CommandEnumerate::new(uuid, model, fields);
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
            field,
            show_fields,
            show_meta,
            show_source,
        } => {
            let instance = CommandGet::new(model, show_fields, show_meta, show_source);
            instance.command(&data, &indexes);
        }
        Commands::Interactive => {
            let mut out = BufWriter::new(stdout().lock());
            write(&mut out, "> ".as_bytes());

            while interactive(&data, &indexes) {
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

fn enumerate(
    data: &Structure,
    indexes: &UuidIndexes,
    show_uuid: bool,
    show_fields: bool,
) -> Vec<String> {
    let mut lines: Vec<String> =
        Vec::with_capacity(indexes.get_models().len() + indexes.get_fields().len());

    for model in &data.models {
        if show_uuid {
            lines.push(format!(
                "[M] {}: {}",
                model._meta_data.uuid, model.object_name
            ));
        } else {
            lines.push(format!("[M] {}", model.object_name));
        }

        if !show_fields {
            continue;
        }

        for field in &model.local_fields {
            if show_uuid {
                lines.push(format!("[F] {}: {}", field._meta_data.uuid, field.name));
            } else {
                lines.push(format!("[F] {}", field.name));
            }
        }
    }

    lines
}

fn get_display_models(model: &MyModel) -> Vec<String> {
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

fn get_display_fields(indexes: &UuidIndexes, field: &MyField, category: FieldType) -> Vec<String> {
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

fn get_display_meta_data(meta_data: &MetaData) -> Vec<String> {
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

fn interactive(data: &Structure, indexes: &UuidIndexes) -> bool {
    let mut read_str = String::new();
    let read = std::io::stdin();
    // if read.is_terminal() {
    //     return true;
    // }

    read.read_line(&mut read_str).ok();
    let mut cmd_args_iter = read_str
        .trim()
        .split_whitespace()
        .map(|e| e.trim().to_string())
        .filter(|s| s.len() > 0);

    // println!("{}", cmd_args_iter.clone().collect::<String>().len());

    let cmd = if let Some(c) = cmd_args_iter.next() {
        c
    } else {
        println!("\r> ");
        return true;
    };

    let args: Vec<String> = cmd_args_iter.collect();
    match cmd.to_lowercase().as_str() {
        "get" => {
            // get
            return true;
        }
        "exit" | "quit" => false,
        _ => {
            let msg = format!("unexpected command: {}", cmd);
            println!("{}", msg);
            return true;
        }
    }
}

trait Command {
    fn command(&self, data: &Arc<Structure>, indexes: &UuidIndexes);
}

struct CommandEnumerate {
    uuid: bool,
    model: Option<String>,
    fields: bool,
}

impl CommandEnumerate {
    pub fn new(uuid: bool, model: Option<String>, fields: bool) -> Self {
        Self {
            uuid,
            model,
            fields,
        }
    }
}

impl Command for CommandEnumerate {
    fn command(&self, data: &Arc<Structure>, indexes: &UuidIndexes) {
        let (data, indexes) = if let Some(ref model_name) = self.model {
            if let Some(model) = get_model_by(&indexes, model_name.as_str()) {
                let uuid = model._meta_data.uuid.clone();
                rebuild(Arc::clone(data), indexes.clone(), uuid)
            } else {
                panic!("no match {} in models", model_name);
            }
        } else {
            (Arc::clone(data), indexes.clone())
        };

        let lines = enumerate(&data, &indexes, self.uuid, self.fields);
        let mut out = BufWriter::new(stdout().lock());
        write(&mut out, lines.join("\n").as_bytes());
        println!("");
    }
}

struct CommandGet {
    model: String,
    show_fields: bool,
    show_meta: bool,
    show_source: bool,
}

impl CommandGet {
    pub fn new(model: String, show_fields: bool, show_meta: bool, show_source: bool) -> Self {
        Self {
            model,
            show_fields,
            show_meta,
            show_source,
        }
    }
}

impl Command for CommandGet {
    fn command(&self, data: &Arc<Structure>, indexes: &UuidIndexes) {
        if let Some(model) = get_model_by(&indexes, self.model.as_str()) {
            let mut lines = get_display_models(&model);
            if self.show_fields {
                for f in model.local_fields.iter() {
                    lines.extend(get_display_fields(&indexes, f, FieldType::Local));
                }

                for f in model.relation_fields.iter() {
                    lines.extend(get_display_fields(&indexes, f, FieldType::Related));
                }

                for f in model.forward_fields.iter() {
                    lines.extend(get_display_fields(&indexes, f, FieldType::Forwarded));
                }
            }

            if self.show_meta {
                lines.extend(get_display_meta_data(&model._meta_data));
            }

            if self.show_source {
                lines.push(model._meta_data.code.partial.concat());
            }

            lines.push("".to_owned());

            let mut out = BufWriter::new(stdout().lock());
            write(&mut out, lines.join("\n").as_bytes());
        } else {
            panic!("no match {} in models", self.model);
        }
    }
}

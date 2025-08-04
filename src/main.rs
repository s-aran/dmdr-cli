use clap::{Parser, Subcommand};
use dmdr_core::model::{MetaData, MyModel};
use std::io::{BufWriter, Write, stdout};
use std::sync::Arc;
use std::{fs::File, path::PathBuf};

use dmdr_core::{
    load_json,
    model::{Structure, UuidIndexes},
};

#[derive(Parser)]
#[clap(author, version, about, long_about = None, subcommand_required = true, arg_required_else_help = true)]
struct Args {
    #[clap(subcommand)]
    command: Commands,
    #[clap(short, long)]
    file: String,
}

#[derive(Subcommand)]
enum Commands {
    Enumerate {
        #[clap(short, long)]
        uuid: bool,
        #[clap(short, long)]
        model: Option<String>,
    },
    Write {
        #[clap(short, long)]
        model: Option<String>,
    },
    Get {
        #[clap(short, long)]
        model: String,
        #[clap(long)]
        show_meta: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let (data, indexes) = load_json(args.file.into())?;

    match args.command {
        Commands::Enumerate { uuid, model } => {
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

            let lines = enumerate(&data, &indexes, uuid);
            let mut out = BufWriter::new(stdout().lock());
            write(&mut out, lines.join("\n").as_bytes());
            println!("");
        }
        Commands::Write { model } => {
            write_dot(&data, &indexes, model, Some("data.dot".into()))?;
        }
        Commands::Get { model, show_meta } => {
            if let Some(model) = get_model_by(&indexes, model.as_str()) {
                show_model(&model, show_meta);
            } else {
                panic!("no match {} in models", model);
            }
        }
    }

    Ok(())

    // and ...
    // > dot -Kdot -Gdpi=300 -Tpng data.dot -odata.png
}

fn get_model_by(indexes: &UuidIndexes, model_name_or_uuid: &str) -> Option<Arc<MyModel>> {
    let specified_uuid = indexes.has_model(model_name_or_uuid);
    let specified_name = indexes.has_model_name(model_name_or_uuid);

    if specified_name {
        Some(indexes.get_model_by_name(model_name_or_uuid))
    } else if specified_uuid {
        Some(indexes.get_model(model_name_or_uuid))
    } else {
        None
    }
}

fn write_dot(
    data: &Structure,
    indexes: &UuidIndexes,
    target_model: Option<String>,
    output_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let dot = dump_er_dot(data, indexes, target_model);

    if let Some(path) = output_path {
        let file = File::create(path)?;
        let mut out = BufWriter::new(file);
        write(&mut out, dot.as_bytes());
    } else {
        let file = stdout();
        let mut out = BufWriter::new(file.lock());
        write(&mut out, dot.as_bytes());
    };

    Ok(())
}

fn dump_er_dot(data: &Structure, indexes: &UuidIndexes, target_model: Option<String>) -> String {
    let mut dot = String::from("digraph ER {\n");

    // define node
    for model in &data.models {
        let uuid = &model._meta_data.uuid;
        if let Some(target) = target_model.as_ref()
            && target != uuid
        {
            continue;
        }

        let label = &model.object_name;
        dot.push_str(&format!("  \"{uuid}\" [label=\"{label}\"];\n"));
    }

    // define edge
    for rel in &data.relations {
        let src_model_uuid = indexes.get_model_from_field(&rel.src_field);
        let dst_model_uuid = &rel.target_model;

        // if let Some(target) = target_model.as_ref()
        //     && (target != src_model_uuid || target != dst_model_uuid)
        // {
        //     continue;
        // }

        let rel_label = format!("{:?}", rel.relation_type);

        dot.push_str(&format!(
            "  \"{src}\" -> \"{dst}\" [label=\"{lbl}\"];\n",
            src = src_model_uuid,
            dst = dst_model_uuid,
            lbl = rel_label
        ));
    }

    dot.push_str("}\n");
    dot
}

fn write<T>(to: &mut BufWriter<T>, data: &[u8])
where
    T: Sized + Write,
{
    let _ = to.write_all(data);
    let _ = to.flush();
}

fn enumerate(data: &Structure, indexes: &UuidIndexes, show_uuid: bool) -> Vec<String> {
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

        for field in &model.fields {
            if show_uuid {
                lines.push(format!("[F] {}: {}", field._meta_data.uuid, field.name));
            } else {
                lines.push(format!("[F] {}", field.name));
            }
        }
    }

    lines
}

fn show_model(model: &MyModel, show_meta: bool) {
    let mut lines = vec![];

    lines.push(format!("model name: {}", model.model_name));
    lines.push(format!("object name: {}", model.object_name));
    lines.push(format!("app label: {}", model.app_label));
    lines.push(format!("db table: {}", model.db_table));
    lines.push(format!("fields: {}", model.fields.len()));
    lines.push("".to_owned());

    let mut out = BufWriter::new(stdout().lock());

    write(&mut out, lines.join("\n").as_bytes());
    if show_meta {
        show_meta_data(&model._meta_data);
    }
}

fn show_meta_data(meta_data: &MetaData) {
    let mut lines = vec![];

    lines.push(format!("uuid: {}", meta_data.uuid));
    lines.push(format!("source file: {}", meta_data.code.source_file));
    lines.push(format!("source line: {}", meta_data.code.line_number));
    lines.push("".to_owned());

    let mut out = BufWriter::new(stdout().lock());

    write(&mut out, lines.join("\n").as_bytes());
}

fn rebuild(
    data: Arc<Structure>,
    indexes: UuidIndexes,
    model_uuid: String,
) -> (Arc<Structure>, UuidIndexes) {
    let model = indexes.get_model(&model_uuid);

    let new_models = vec![model.clone()];

    // TODO: M2M
    let new_relations = data
        .relations
        .iter()
        .filter(|rel| rel.target_model == model_uuid)
        .map(|rel| rel.clone())
        .collect::<Vec<_>>();

    let new_data = Structure {
        models: new_models
            .iter()
            .map(|arc_model| Arc::clone(arc_model).as_ref().clone())
            .collect(),
        relations: new_relations,
    };

    let shared = Arc::new(new_data);
    let new_indexes = UuidIndexes::new(&shared.clone());

    (shared, new_indexes)
}

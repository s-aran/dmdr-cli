use ahash::AHashSet;
use clap::{Parser, Subcommand};
use dmdr_core::model::{MetaData, MyField, MyModel, RelationType};
use std::io::{stdout, BufWriter, Write};
use std::sync::Arc;
use std::{fs::File, path::PathBuf};

use dmdr_core::{
    load_json,
    model::{Structure, UuidIndexes},
};

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
            if let Some(model) = get_model_by(&indexes, model.as_str()) {
                let mut lines = get_display_models(&model);
                if show_fields {
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

                if show_meta {
                    lines.extend(get_display_meta_data(&model._meta_data));
                }

                if show_source {
                    lines.push(model._meta_data.code.partial.concat());
                }

                lines.push("".to_owned());

                let mut out = BufWriter::new(stdout().lock());
                write(&mut out, lines.join("\n").as_bytes());
            } else {
                panic!("no match {} in models", model);
            }
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

fn write_dot(
    data: &Structure,
    indexes: &UuidIndexes,
    output_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let dot = dump_er_dot(data, indexes);

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

    // 指定モデルに関連する全リレーション（src/target/through いずれか一致）
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

    // 関与する全モデルUUIDを収集（src/target/through）
    for rel in &new_relations {
        keep_models.insert(rel.src_model_uuid.clone());
        keep_models.insert(rel.target_model_uuid.clone());
        if let Some(t) = &rel.through_model_uuid {
            keep_models.insert(t.clone());
        }
    }

    // 上で集めたモデルのみ残す
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

fn dump_er_dot(data: &Structure, indexes: &UuidIndexes) -> String {
    use std::collections::BTreeMap;
    use std::fmt::Write;

    fn default_jp_font() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "Yu Gothic,Meiryo,Noto Sans CJK JP,Segoe Emoji"
        }

        #[cfg(target_os = "macos")]
        {
            "Hiragino Sans,Noto Sans CJK JP,Apple Color Emoji"
        }

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            "Noto Sans CJK JP,Noto Sans,DejaVu Sans"
        }
    }

    fn escape(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 8);
        for ch in s.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => {}
                _ => out.push(ch),
            }
        }
        out
    }

    fn format_field_lines(model: &MyModel, max_fields: usize) -> String {
        let mut lines = Vec::new();
        for f in model.local_fields.iter().take(max_fields) {
            let mut flags = Vec::new();
            if !f.null {
                flags.push("NOT NULL");
            }
            if !f.verbose_name.is_empty() {
                flags.push(f.verbose_name.as_str());
            }

            let flag = if flags.is_empty() {
                "".to_string()
            } else {
                format!(" ({})", flags.join(", "))
            };
            lines.push(format!("• {}{}", f.name, flag));
        }
        if model.local_fields.len() > max_fields {
            lines.push(format!(
                "… and {} more",
                model.local_fields.len() - max_fields
            ));
        }
        lines.join("\\n")
    }

    fn card(rt: &RelationType) -> (&'static str, &'static str) {
        match rt {
            RelationType::ForeignKey => ("N", "1"),
            RelationType::OneToOne => ("1", "1"),
            RelationType::ManyToMany => ("N", "N"),
        }
    }

    let font = default_jp_font();

    let mut out = String::new();
    writeln!(
        out,
        "digraph ER {{\n\
         graph [rankdir=LR, charset=\"UTF-8\", fontname=\"{font}\", fontsize=12];\n\
         node  [shape=record,       fontname=\"{font}\", fontsize=11];\n\
         edge  [                    fontname=\"{font}\", fontsize=10, arrowsize=0.8, labeldistance=1.2, labelfontsize=9];",
    ).unwrap();

    // app_label ごとにクラスタ化（順序安定）
    let mut apps: BTreeMap<&str, Vec<&MyModel>> = BTreeMap::new();
    for m in &data.models {
        apps.entry(m.app_label.as_str()).or_default().push(m);
    }

    // ノード
    for (i, (app, models)) in apps.iter().enumerate() {
        writeln!(
            out,
            "  subgraph cluster_{} {{\n    label = \"{}\";\n    style=rounded;\n    color=\"#aaaaaa\";",
            i, escape(app)
        ).unwrap();
        for m in models {
            let node_id = escape(&m._meta_data.uuid);
            let title = format!("{}|<name>{}", escape(&m.object_name), escape(&m.model_name));
            let fields = format_field_lines(m, 8);
            let fields = if fields.is_empty() {
                "".to_string()
            } else {
                format!("|{}", fields)
            };
            writeln!(
                out,
                "    \"{}\" [label=\"{{{}}}{}\"];",
                node_id, title, fields
            )
            .unwrap();
        }
        writeln!(out, "  }}").unwrap();
    }
    writeln!(out).unwrap();

    // エッジ
    for rel in &data.relations {
        // ここが重要：**必ず UUID を使う**（名前やフィールド名では引けないことがある）
        let src_uuid = &rel.src_model_uuid;
        let dst_uuid = &rel.target_model_uuid;

        // 表示用に名前解決（無ければUUID）
        let src_name = indexes
            .get_model_by_uuid(src_uuid)
            .map(|m| m.object_name.clone())
            .unwrap_or_else(|| src_uuid.clone());
        let dst_name = indexes
            .get_model_by_uuid(dst_uuid)
            .map(|m| m.object_name.clone())
            .unwrap_or_else(|| dst_uuid.clone());

        match rel.relation_type {
            RelationType::ManyToMany => {
                if let Some(thr_uuid) = &rel.through_model_uuid {
                    // through 指定時は破線で 2 本に分けて描画
                    writeln!(
                        out,
                        "  \"{}\" -> \"{}\" [style=dashed, taillabel=\"N\", headlabel=\"N\", tooltip=\"{} <-> {} via {}\"];\n  \"{}\" -> \"{}\" [style=dashed, taillabel=\"N\", headlabel=\"N\", tooltip=\"{} <-> {} via {}\"];",
                        escape(src_uuid),
                        escape(thr_uuid),
                        escape(&src_name), escape(&dst_name), escape(rel.through_model.as_deref().unwrap_or("")),
                        escape(thr_uuid),
                        escape(dst_uuid),
                        escape(&src_name), escape(&dst_name), escape(rel.through_model.as_deref().unwrap_or("")),
                    ).unwrap();
                } else {
                    writeln!(
                        out,
                        "  \"{}\" -> \"{}\" [taillabel=\"N\", headlabel=\"N\", tooltip=\"{} <-> {} (ManyToMany)\"];",
                        escape(src_uuid), escape(dst_uuid), escape(&src_name), escape(&dst_name)
                    ).unwrap();
                }
            }
            _ => {
                let (tail, head) = card(&rel.relation_type);
                let mut attrs = format!(
                    "taillabel=\"{}\", headlabel=\"{}\", tooltip=\"{}.{} -> {} ({:?})\"",
                    tail,
                    head,
                    escape(&src_name),
                    escape(&rel.src_field),
                    escape(&dst_name),
                    rel.relation_type
                );
                if matches!(rel.relation_type, RelationType::OneToOne) {
                    attrs.push_str(", dir=both, arrowhead=normal, arrowtail=normal");
                }
                writeln!(
                    out,
                    "  \"{}\" -> \"{}\" [{}];",
                    escape(src_uuid),
                    escape(dst_uuid),
                    attrs
                )
                .unwrap();
            }
        }
    }

    writeln!(out, "}}").unwrap();
    out
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
            //
            return true;
        }
        "exit" | "quit" => false,
        _ => {
            println!("");
            let msg = format!("unexpected command: {}", cmd);
            println!("{}", msg);
            return true;
        }
    }
}

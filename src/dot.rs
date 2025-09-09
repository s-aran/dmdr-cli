use std::{
    fs::File,
    io::{BufWriter, stdout},
    path::PathBuf,
};

use dmdr_core::model::{MyModel, RelationType, Structure, UuidIndexes};

use crate::write;

pub fn write_dot(
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

            if !f.help_text.is_empty() {
                flags.push(f.help_text.as_str());
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

    let mut apps: BTreeMap<&str, Vec<&MyModel>> = BTreeMap::new();
    for m in &data.models {
        apps.entry(m.app_label.as_str()).or_default().push(m);
    }

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

    for rel in &data.relations {
        let src_uuid = &rel.src_model_uuid;
        let dst_uuid = &rel.target_model_uuid;

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

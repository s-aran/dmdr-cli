use std::{
    io::{BufWriter, stdout},
    sync::Arc,
};

use crate::{FieldType, get_display_fields, get_display_meta_data, get_display_models, write};
use dmdr_core::model::{Structure, UuidIndexes};

use crate::{get_model_by, rebuild};

pub trait Command {
    fn command(&mut self, data: &Arc<Structure>, indexes: &UuidIndexes);
}

pub struct CommandEnumerate {
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
    fn command(&mut self, data: &Arc<Structure>, indexes: &UuidIndexes) {
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

pub struct CommandInteractive {
    pub continuous: bool,
}

impl CommandInteractive {
    pub fn new() -> Self {
        Self { continuous: false }
    }

    pub fn is_continuous(&self) -> bool {
        self.continuous
    }
}

impl Command for CommandInteractive {
    fn command(&mut self, data: &Arc<Structure>, indexes: &UuidIndexes) {
        self.continuous = interactive(data, indexes);
    }
}

pub struct CommandGet {
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
    fn command(&mut self, data: &Arc<Structure>, indexes: &UuidIndexes) {
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

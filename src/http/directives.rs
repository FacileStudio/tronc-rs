use super::csp::{Directive, Sources};

pub fn extend(directives: Vec<Directive>, extra: &Sources, add_new: bool) -> Vec<Directive> {
    let known: Vec<String> = directives.iter().map(|d| d.name.clone()).collect();
    let mut merged: Vec<Directive> = directives
        .into_iter()
        .map(|d| Directive { sources: merge(d.sources, extra.get(&d.name)), name: d.name })
        .collect();
    if add_new {
        let added = extra.iter().filter(|(name, _)| !known.contains(name));
        merged.extend(
            added.map(|(name, sources)| Directive { name: name.clone(), sources: merge(Vec::new(), Some(sources)) }),
        );
    }
    merged
}

fn merge(defaults: Vec<String>, extra: Option<&Vec<String>>) -> Vec<String> {
    let Some(extra) = extra.filter(|extra| !extra.is_empty()) else { return defaults };
    let mut merged = if defaults.len() == 1 && defaults[0] == "'none'" { Vec::new() } else { defaults };
    for source in extra {
        if !merged.contains(source) {
            merged.push(source.clone());
        }
    }
    merged
}

pub fn parse(policy: &str) -> Vec<Directive> {
    policy
        .split(';')
        .filter_map(|part| {
            let mut fields = part.split_whitespace();
            let name = fields.next()?.to_ascii_lowercase();
            Some(Directive { name, sources: fields.map(str::to_owned).collect() })
        })
        .collect()
}

pub fn render(directives: &[Directive]) -> String {
    let parts: Vec<String> = directives
        .iter()
        .map(|d| if d.sources.is_empty() { d.name.clone() } else { format!("{} {}", d.name, d.sources.join(" ")) })
        .collect();
    parts.join("; ")
}

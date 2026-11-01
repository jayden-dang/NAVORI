use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Machine {
    pub model: String,
    pub macos: String,
}

#[derive(Debug)]
pub struct Baseline {
    pub machine: Machine,
    pub revision: String,
    pub recorded: String,
    pub metrics: BTreeMap<String, (f64, f64)>,
}

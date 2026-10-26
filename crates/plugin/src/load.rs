//! A load order: plugins in the order the game loads them, and which copy of
//! a form wins (the last plugin to hold it).

use std::collections::HashMap;

use crate::records::Typed;
use crate::{FormRef, Plugin, PluginError, Record};

#[derive(Debug, Default)]
pub struct LoadOrder {
    pub plugins: Vec<Plugin>,
    /// For each form, the plugin that holds the winning copy.
    winners: HashMap<FormRef, usize>,
}

impl LoadOrder {
    /// `plugins` in load order.
    pub fn new(plugins: Vec<Plugin>) -> Self {
        let mut winners = HashMap::new();
        for (i, p) in plugins.iter().enumerate() {
            for (form, _) in p.forms() {
                winners.insert(form, i);
            }
        }
        LoadOrder { plugins, winners }
    }

    /// The plugin by file name, compared without case.
    pub fn plugin(&self, name: &str) -> Option<&Plugin> {
        self.plugins
            .iter()
            .find(|p| p.name().eq_ignore_ascii_case(name))
    }

    /// The winning copy of a form and the plugin it is in.
    pub fn winner(&self, form: &FormRef) -> Option<(&Plugin, &Record)> {
        let p = &self.plugins[*self.winners.get(form)?];
        Some((p, p.get(form)?))
    }

    /// The winning copy, read as `T`. `None` when no plugin holds the form,
    /// or the winner is a record of another type or deleted.
    pub fn get<T: Typed>(&self, form: &FormRef) -> Option<Result<T, PluginError>> {
        let (p, r) = self.winner(form)?;
        (r.kind == T::KIND && !r.deleted()).then(|| T::read(r, &p.names))
    }

    /// Every form with a winner.
    pub fn forms(&self) -> impl Iterator<Item = &FormRef> {
        self.winners.keys()
    }

    /// Every plugin that holds a form, in load order.
    pub fn copies(&self, form: &FormRef) -> Vec<&Plugin> {
        self.plugins
            .iter()
            .filter(|p| p.get(form).is_some())
            .collect()
    }
}

//! The installed tools. This is the only place that names them: to add a
//! tool, create `src/tools/<name>/` implementing [`crate::core::tool::Tool`]
//! and add one line below. Tools must not import each other; shared code
//! belongs in `crate::kit`.

pub mod claude;
pub mod cleanup;
pub mod lens;
pub mod system;

use crate::core::tool::{Tool, ToolEnv};

/// Every tool, in the order of the window sidebar.
pub fn all(env: &ToolEnv) -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(system::System::new(env)),
        Box::new(cleanup::Cleanup::new(env)),
        Box::new(lens::Lens::new(env)),
        Box::new(claude::Claude::new(env)),
    ]
}

/// The panel's widgets on first launch, as `tool/widget` keys. Widgets not
/// listed follow in tool order. The user can change this in the panel.
pub const DEFAULT_WIDGET_ORDER: &[&str] = &[
    "cleanup/clean",
    "core/planned",
    "system/storage",
    "system/memory",
    "system/cpu",
    "system/battery",
    "system/network",
    "system/devices",
    "claude/limits",
    "core/recommendation",
];

#[cfg(test)]
mod tests {
    #[test]
    fn tool_ids_and_widget_ids_are_unique() {
        let env = crate::core::tool::ToolEnv { ctx: egui::Context::default(), demo: true };
        let tools = super::all(&env);
        let mut ids: Vec<&str> = tools.iter().map(|t| t.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), tools.len());
        assert!(!ids.contains(&crate::core::shell::CORE));
        for t in &tools {
            let mut w: Vec<&str> = t.widgets().iter().map(|w| w.id).collect();
            w.sort();
            let n = w.len();
            w.dedup();
            assert_eq!(w.len(), n, "duplicate widget id in {}", t.id());
            for spec in t.widgets() {
                assert!((1..=crate::core::tool::GRID_COLUMNS).contains(&spec.span));
                assert!(t.widget_height(spec.id, 200.0) > 0.0, "{}/{} has no height", t.id(), spec.id);
            }
        }
    }

    /// Tools stay isolated: no tool module refers to another tool's module.
    #[test]
    fn tools_do_not_import_each_other() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools");
        let names: Vec<String> = std::fs::read_dir(&root).unwrap().flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        for me in &names {
            for f in std::fs::read_dir(root.join(me)).unwrap().flatten() {
                let src = std::fs::read_to_string(f.path()).unwrap();
                for other in names.iter().filter(|o| *o != me) {
                    for pat in [format!("tools::{other}"), format!("super::super::{other}")] {
                        assert!(!src.contains(&pat), "{} uses {pat}", f.path().display());
                    }
                }
            }
        }
    }
}

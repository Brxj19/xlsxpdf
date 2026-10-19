use std::collections::HashMap;

use crate::metrics::FontMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

#[derive(Debug, Clone)]
pub struct ResolvedFont {
    pub id: FontId,
    pub family: String,
    pub metrics: FontMetrics,
    pub bold: bool,
    pub italic: bool,
}

/// Deterministic font registry.
///
/// Resolution chain (per architecture contract 03):
/// 1. Exact configured font
/// 2. Metric-compatible substitute
/// 3. Script-appropriate fallback
/// 4. Final Unicode fallback
///
/// In v1 (pre-M6), this uses built-in metric tables. After FreeType
/// integration, it will resolve against actual font files.
pub struct FontRegistry {
    fonts: Vec<ResolvedFont>,
    name_map: HashMap<String, FontId>,
    fallback_id: FontId,
}

impl FontRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            fonts: Vec::new(),
            name_map: HashMap::new(),
            fallback_id: FontId(0),
        };

        // Register built-in metric-compatible fonts
        reg.register("Calibri", FontMetrics::calibri(), false, false);
        reg.register("Arial", FontMetrics::arial(), false, false);
        reg.register("Times New Roman", FontMetrics::times_new_roman(), false, false);
        reg.register("Helvetica", FontMetrics::arial(), false, false);

        // Common substitutions
        let calibri_id = reg.name_map["Calibri"];
        reg.name_map.insert("Calibri Light".to_string(), calibri_id);
        let arial_id = reg.name_map["Arial"];
        reg.name_map.insert("Arial Narrow".to_string(), arial_id);
        reg.name_map.insert("Helvetica Neue".to_string(), arial_id);
        reg.name_map.insert("sans-serif".to_string(), arial_id);
        let tnr_id = reg.name_map["Times New Roman"];
        reg.name_map.insert("Times".to_string(), tnr_id);
        reg.name_map.insert("serif".to_string(), tnr_id);

        reg.fallback_id = calibri_id;

        reg
    }

    fn register(
        &mut self,
        name: &str,
        metrics: FontMetrics,
        bold: bool,
        italic: bool,
    ) -> FontId {
        let id = FontId(self.fonts.len() as u32);
        self.fonts.push(ResolvedFont {
            id,
            family: name.to_string(),
            metrics,
            bold,
            italic,
        });
        self.name_map.insert(name.to_string(), id);
        id
    }

    pub fn resolve(&self, name: &str, _bold: bool, _italic: bool) -> &ResolvedFont {
        // Try exact match
        if let Some(&id) = self.name_map.get(name) {
            return &self.fonts[id.0 as usize];
        }

        // Try case-insensitive
        let lower = name.to_lowercase();
        for (registered, &id) in &self.name_map {
            if registered.to_lowercase() == lower {
                return &self.fonts[id.0 as usize];
            }
        }

        // Fallback
        &self.fonts[self.fallback_id.0 as usize]
    }

    pub fn get(&self, id: FontId) -> &ResolvedFont {
        &self.fonts[id.0 as usize]
    }

    pub fn fallback(&self) -> &ResolvedFont {
        &self.fonts[self.fallback_id.0 as usize]
    }

    pub fn font_count(&self) -> usize {
        self.fonts.len()
    }
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_exact() {
        let reg = FontRegistry::new();
        let font = reg.resolve("Calibri", false, false);
        assert_eq!(font.family, "Calibri");
    }

    #[test]
    fn test_resolve_substitute() {
        let reg = FontRegistry::new();
        let font = reg.resolve("Helvetica", false, false);
        assert_eq!(font.metrics.units_per_em, FontMetrics::arial().units_per_em);
    }

    #[test]
    fn test_resolve_fallback() {
        let reg = FontRegistry::new();
        let font = reg.resolve("UnknownFont", false, false);
        assert_eq!(font.family, "Calibri");
    }

    #[test]
    fn test_metrics_calculation() {
        let m = FontMetrics::calibri();
        let width = m.char_width_pt(11.0);
        assert!(width > 0.0);
        let height = m.line_height_pt(11.0);
        assert!(height > 0.0);
    }
}

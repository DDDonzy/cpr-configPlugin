use indexmap::IndexMap;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, MapAccess, Visitor},
};
use std::{collections::BTreeSet, fmt};

pub const MAX_BYTES: usize = 1024 * 1024;
pub const MAX_MODELS: usize = 2000;
const MAX_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub display_name: String,
    pub reasoning_levels: Vec<String>,
    pub context_window: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null_output"
    )]
    pub max_output_tokens: Option<u64>,
    pub input_modalities: Vec<String>,
}
fn non_null_output<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    u64::deserialize(d).map(Some)
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(transparent)]
pub struct Catalog(pub IndexMap<String, Model>);
impl PartialEq for Catalog {
    fn eq(&self, other: &Self) -> bool {
        self.0.iter().eq(other.0.iter())
    }
}
impl Eq for Catalog {}
impl<'de> Deserialize<'de> for Catalog {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct CatalogVisitor;
        impl<'de> Visitor<'de> for CatalogVisitor {
            type Value = Catalog;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("model-ID object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut result = IndexMap::new();
                while let Some((key, value)) = map.next_entry::<String, Model>()? {
                    if result.insert(key.clone(), value).is_some() {
                        return Err(de::Error::custom(format!("模型 ID 重复：{key}")));
                    }
                    if result.len() > MAX_MODELS {
                        return Err(de::Error::custom("模型数量过多"));
                    }
                }
                Ok(Catalog(result))
            }
        }
        d.deserialize_map(CatalogVisitor)
    }
}
impl Catalog {
    pub fn ordered(&self, ids: &[String]) -> Result<Self, String> {
        if ids.len() != self.0.len() {
            return Err("模型顺序与模型集合不一致".into());
        }
        let mut result = IndexMap::new();
        for id in ids {
            let model = self
                .0
                .get(id)
                .ok_or_else(|| "顺序包含不存在的模型".to_string())?;
            if result.insert(id.clone(), model.clone()).is_some() {
                return Err("模型顺序重复".into());
            }
        }
        Ok(Self(result))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.0.len() > MAX_MODELS {
            return Err("模型数量过多".into());
        }
        for (id, m) in &self.0 {
            let invalid = |reason: &str| format!("{id}：{reason}");
            if id.is_empty()
                || id.len() > 256
                || id.trim() != id
                || id.chars().any(|c| c.is_control() || c.is_whitespace())
                || id.contains('*')
                || id.starts_with("__")
                || matches!(id.as_str(), "__proto__" | "constructor" | "prototype")
            {
                return Err(invalid("模型 ID 无效"));
            }
            if m.display_name.trim() != m.display_name
                || m.display_name.trim().is_empty()
                || m.display_name.len() > 512
                || m.display_name.chars().any(char::is_control)
            {
                return Err(invalid("显示名称无效"));
            }
            if m.context_window == 0 || m.context_window > MAX_INTEGER {
                return Err(invalid("上下文必须是正整数"));
            }
            if m.max_output_tokens
                .is_some_and(|n| n == 0 || n > MAX_INTEGER)
            {
                return Err(invalid("输出上限必须是正整数，未知时不填写"));
            }
            if m.reasoning_levels.len() > 16 {
                return Err(invalid("思考等级过多"));
            }
            let mut levels = BTreeSet::new();
            for level in &m.reasoning_levels {
                if !matches!(
                    level.as_str(),
                    "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
                ) {
                    return Err(invalid("不支持的基础思考等级；ultra 属于 Codex 客户端策略"));
                }
                if !levels.insert(level) {
                    return Err(invalid("思考等级重复"));
                }
            }
            if m.input_modalities.is_empty() || m.input_modalities.len() > 4 {
                return Err(invalid("至少选择一种输入类型"));
            }
            let mut modalities = BTreeSet::new();
            for modality in &m.input_modalities {
                if !matches!(modality.as_str(), "text" | "image" | "audio" | "video")
                    || !modalities.insert(modality)
                {
                    return Err(invalid("输入类型无效或重复"));
                }
            }
        }
        if self.render()?.len() > MAX_BYTES {
            return Err("配置文件过大".into());
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("配置文件过大".into());
        }
        let catalog: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("JSON 格式或字段无效：{e}"))?;
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn render(&self) -> Result<String, String> {
        let mut blocks = Vec::new();
        for (id, m) in &self.0 {
            let quote = |v: &str| serde_json::to_string(v).map_err(|_| "JSON 编码失败".to_string());
            let levels = serde_json::to_string(&m.reasoning_levels).map_err(|_| "JSON 编码失败")?;
            let inputs = serde_json::to_string(&m.input_modalities).map_err(|_| "JSON 编码失败")?;
            let output = m
                .max_output_tokens
                .map(|v| format!("\n    \"max_output_tokens\": {v},"))
                .unwrap_or_default();
            blocks.push(format!("  {}: {{\n    \"display_name\": {},\n    \"reasoning_levels\": {},\n    \"context_window\": {},{}\n    \"input_modalities\": {}\n  }}",quote(id)?,quote(&m.display_name)?,levels.replace("\",\"","\", \""),m.context_window,output,inputs.replace("\",\"","\", \"")));
        }
        Ok(if blocks.is_empty() {
            "{}\n".into()
        } else {
            format!("{{\n{}\n}}\n", blocks.join(",\n"))
        })
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub order_changed: bool,
}
pub fn diff(before: &Catalog, after: &Catalog) -> Diff {
    Diff {
        order_changed: before
            .0
            .keys()
            .filter(|id| after.0.contains_key(*id))
            .ne(after.0.keys().filter(|id| before.0.contains_key(*id))),
        added: after
            .0
            .keys()
            .filter(|id| !before.0.contains_key(*id))
            .cloned()
            .collect(),
        removed: before
            .0
            .keys()
            .filter(|id| !after.0.contains_key(*id))
            .cloned()
            .collect(),
        changed: after
            .0
            .iter()
            .filter(|(id, m)| before.0.get(*id).is_some_and(|old| old != *m))
            .map(|(id, _)| id.clone())
            .collect(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn bytes() -> Vec<u8> {
        br#"{"demo":{"display_name":"Demo","reasoning_levels":["low","high"],"context_window":1000,"max_output_tokens":100,"input_modalities":["text","image"]}}"#.to_vec()
    }
    #[test]
    fn render_round_trip() {
        let c = Catalog::parse(&bytes()).unwrap();
        let t = c.render().unwrap();
        assert_eq!(Catalog::parse(t.as_bytes()).unwrap(), c);
        assert!(t.contains("[\"low\", \"high\"]"));
        assert!(!t.contains("default_reasoning"));
    }
    #[test]
    fn duplicates_rejected() {
        let b = bytes();
        let m = std::str::from_utf8(&b).unwrap();
        let entry = &m[1..m.len() - 1];
        assert!(Catalog::parse(format!("{{{entry},{entry}}}").as_bytes()).is_err());
    }
    #[test]
    fn client_fields_rejected() {
        let mut v: serde_json::Value = serde_json::from_slice(&bytes()).unwrap();
        v["demo"]["default_reasoning"] = "low".into();
        assert!(Catalog::parse(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    #[test]
    fn token_validation() {
        let mut c = Catalog::parse(&bytes()).unwrap();
        c.0.get_mut("demo").unwrap().context_window = 0;
        assert!(c.validate().is_err());
    }
    #[test]
    fn ultra_rejected() {
        let mut c = Catalog::parse(&bytes()).unwrap();
        c.0.get_mut("demo")
            .unwrap()
            .reasoning_levels
            .push("ultra".into());
        assert!(c.validate().is_err());
    }
    #[test]
    fn unknown_output_is_omitted() {
        let mut c = Catalog::parse(&bytes()).unwrap();
        c.0.get_mut("demo").unwrap().max_output_tokens = None;
        assert!(!c.render().unwrap().contains("max_output_tokens"));
        assert_eq!(Catalog::parse(c.render().unwrap().as_bytes()).unwrap(), c);
    }
    #[test]
    fn empty_catalog_allowed() {
        assert_eq!(Catalog::parse(b"{}").unwrap(), Catalog::default());
    }
    #[test]
    fn order_survives_parse_render_and_serialization() {
        let one = Catalog::parse(&bytes()).unwrap().0["demo"].clone();
        let mut map = IndexMap::new();
        map.insert("z".into(), one.clone());
        map.insert("a".into(), one);
        let c = Catalog(map);
        assert_eq!(Catalog::parse(c.render().unwrap().as_bytes()).unwrap(), c);
        let value = serde_json::json!({"models":c});
        assert_eq!(
            value["models"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["z", "a"]
        );
    }
    #[test]
    fn reorder_is_not_a_noop() {
        let one = Catalog::parse(&bytes()).unwrap().0["demo"].clone();
        let mut map = IndexMap::new();
        map.insert("z".into(), one.clone());
        map.insert("a".into(), one);
        let c = Catalog(map);
        let after = c.ordered(&["a".into(), "z".into()]).unwrap();
        assert_ne!(c, after);
        let change = diff(&c, &after);
        assert!(change.order_changed);
        assert!(change.changed.is_empty());
        assert!(change.added.is_empty());
        assert!(change.removed.is_empty());
    }
    #[test]
    fn invalid_order_rejected() {
        let c = Catalog::parse(&bytes()).unwrap();
        assert!(c.ordered(&[]).is_err());
        assert!(c.ordered(&["missing".into()]).is_err());
    }
}

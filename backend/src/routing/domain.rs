//! 校验精确模型权限与可复用分组，保留原生模型匹配和数量边界

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_MODELS: usize = 256;
pub const MAX_GROUPS: usize = 32;
pub const MAX_DOCUMENT_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    All,
    Allowlist,
    Denylist,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAccess {
    pub mode: Mode,
    pub models: Vec<String>,
}

pub fn valid_model_id(id: &str) -> bool {
    !id.is_empty()
        && id.trim() == id
        && id.len() <= 256
        && !id.starts_with("__")
        && !id.contains('*')
        && !id.chars().any(char::is_control)
}
pub fn normalize_models(models: &[String], allow_empty: bool) -> Result<Vec<String>, &'static str> {
    normalize_models_with_limit(models, allow_empty, true)
}
fn normalize_models_with_limit(
    models: &[String],
    allow_empty: bool,
    enforce_limit: bool,
) -> Result<Vec<String>, &'static str> {
    if models.iter().any(|id| !valid_model_id(id)) {
        return Err("模型 ID 无效：字符格式或长度不兼容，不含通配符");
    }
    let models = models
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if enforce_limit && models.len() > MAX_MODELS {
        return Err("单个名单最多 256 个精确模型 ID");
    }
    if !allow_empty && models.is_empty() {
        return Err("白名单或黑名单至少需要一个模型");
    }
    Ok(models)
}
impl ModelAccess {
    pub fn normalized(&self) -> Result<Self, &'static str> {
        self.normalized_with_limit(true)
    }
    fn normalized_with_limit(&self, enforce_limit: bool) -> Result<Self, &'static str> {
        if self.mode == Mode::All {
            if !self.models.is_empty() {
                return Err("不限制模式不能携带模型名单");
            }
            return Ok(Self::default());
        }
        Ok(Self {
            mode: self.mode,
            models: normalize_models_with_limit(&self.models, false, enforce_limit)?,
        })
    }
    pub fn equivalent(&self, other: &Self) -> bool {
        matches!((self.normalized(), other.normalized()), (Ok(a), Ok(b)) if a == b)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelGroup {
    pub id: String,
    pub name: String,
    pub models: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub group_ids: Vec<String>,
    pub manual_models: Vec<String>,
    pub applied_policy: ModelAccess,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Document {
    pub groups: Vec<ModelGroup>,
    pub bindings: BTreeMap<String, Binding>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupState {
    pub version: Option<u64>,
    pub value: Document,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub enabled: bool,
    pub status: String,
    pub model_access: ModelAccess,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicyRequest {
    pub account_id: String,
    pub before: ModelAccess,
    pub after: ModelAccess,
    pub group_ids: Vec<String>,
    pub manual_models: Vec<String>,
    pub expected_group_version: Option<u64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyDiff {
    pub account_id: String,
    pub account_name: String,
    pub before: ModelAccess,
    pub after: ModelAccess,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub mode_changed: bool,
}

pub fn valid_account_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub fn validate_groups(groups: &[ModelGroup]) -> Result<(), &'static str> {
    if groups.len() > MAX_GROUPS {
        return Err("最多保存 32 个模型分组");
    }
    let mut ids = BTreeSet::new();
    for group in groups {
        if !valid_account_id(&group.id) || !ids.insert(&group.id) {
            return Err("分组 ID 无效或重复");
        }
        if group.name.trim().is_empty()
            || group.name.len() > 192
            || group.name.chars().any(char::is_control)
        {
            return Err("分组名称为空或过长");
        }
        normalize_models(&group.models, true)?;
    }
    Ok(())
}
impl Document {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_groups(&self.groups)?;
        if self.bindings.len() > 512 {
            return Err("分组关联最多保存 512 个账号");
        }
        for (id, b) in &self.bindings {
            if !valid_account_id(id) || b.group_ids.len() > MAX_GROUPS {
                return Err("账号分组关联无效");
            }
            normalize_models(&b.manual_models, true)?;
            b.applied_policy.normalized()?;
        }
        if serde_json::to_vec(self)
            .map_err(|_| "分组序列化失败")?
            .len()
            > MAX_DOCUMENT_BYTES
        {
            return Err("分组及关联数据最多 256 KiB");
        }
        Ok(())
    }
}
pub fn resolve_policy(
    request: &PolicyRequest,
    document: &Document,
) -> Result<ModelAccess, &'static str> {
    resolve_policy_with_limit(request, document, true)
}
pub fn resolve_preview_policy(
    request: &PolicyRequest,
    document: &Document,
) -> Result<ModelAccess, &'static str> {
    resolve_policy_with_limit(request, document, false)
}
fn resolve_policy_with_limit(
    request: &PolicyRequest,
    document: &Document,
    enforce_limit: bool,
) -> Result<ModelAccess, &'static str> {
    if !valid_account_id(&request.account_id) || request.group_ids.len() > MAX_GROUPS {
        return Err("账号或分组参数无效");
    }
    request.before.normalized()?;
    let after = request.after.normalized_with_limit(enforce_limit)?;
    let manual = normalize_models_with_limit(&request.manual_models, true, enforce_limit)?;
    if after.mode == Mode::All {
        if !manual.is_empty() || !request.group_ids.is_empty() {
            return Err("不限制模式不使用模型或分组");
        }
        return Ok(after);
    }
    let mut ids = manual.into_iter().collect::<BTreeSet<_>>();
    let mut selected = BTreeSet::new();
    for group_id in &request.group_ids {
        if !selected.insert(group_id) {
            return Err("分组不能重复选择");
        }
        let group = document
            .groups
            .iter()
            .find(|g| &g.id == group_id)
            .ok_or("所选分组已删除，请重新加载")?;
        ids.extend(group.models.iter().cloned());
    }
    let resolved = ModelAccess {
        mode: after.mode,
        models: ids.into_iter().collect(),
    }
    .normalized_with_limit(enforce_limit)?;
    if resolved != after {
        return Err("最终名单与分组展开结果不一致，请重新预览");
    }
    Ok(after)
}
pub fn diff(account: &Account, after: ModelAccess) -> Result<PolicyDiff, &'static str> {
    let before = account.model_access.normalized()?;
    let old = before.models.iter().cloned().collect::<BTreeSet<_>>();
    let new = after.models.iter().cloned().collect::<BTreeSet<_>>();
    Ok(PolicyDiff {
        account_id: account.id.clone(),
        account_name: account.name.clone(),
        mode_changed: before.mode != after.mode,
        added: new.difference(&old).cloned().collect(),
        removed: old.difference(&new).cloned().collect(),
        before,
        after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy(mode: Mode, models: &[&str]) -> ModelAccess {
        ModelAccess {
            mode,
            models: models.iter().map(|x| x.to_string()).collect(),
        }
    }
    #[test]
    fn native_modes_and_empty_constraints() {
        assert!(policy(Mode::All, &[]).normalized().is_ok());
        assert!(policy(Mode::All, &["gpt"]).normalized().is_err());
        assert!(policy(Mode::Denylist, &[]).normalized().is_err());
        assert!(policy(Mode::Allowlist, &[]).normalized().is_err());
    }
    #[test]
    fn exact_unknown_models_allowed() {
        assert!(
            policy(Mode::Allowlist, &["my-image-model"])
                .normalized()
                .is_ok()
        );
        assert!(!valid_model_id("claude-*"));
        assert!(!valid_model_id("__private"));
        assert!(!valid_model_id(" a"));
        assert!(!valid_model_id("a\n"));
        assert!(valid_model_id("claude-opus-5.5"));
    }
    #[test]
    fn dedup_preserves_case_and_stable_order() {
        assert_eq!(
            policy(Mode::Denylist, &["b", "a", "b", "A"])
                .normalized()
                .unwrap()
                .models,
            vec!["A", "a", "b"]
        );
    }
    #[test]
    fn utf8_byte_limits_and_model_counts() {
        assert!(valid_model_id(&"中".repeat(85)));
        assert!(!valid_model_id(&"中".repeat(86)));
        assert!(
            normalize_models(&(0..256).map(|x| x.to_string()).collect::<Vec<_>>(), false).is_ok()
        );
        assert!(
            normalize_models(&(0..257).map(|x| x.to_string()).collect::<Vec<_>>(), false).is_err()
        );
    }
    #[test]
    fn group_expansion_validated_and_not_wildcarded() {
        let doc = Document {
            groups: vec![ModelGroup {
                id: "g1".into(),
                name: "Claude".into(),
                models: vec!["claude-one".into()],
            }],
            ..Default::default()
        };
        let mut req = PolicyRequest {
            account_id: "acct_1".into(),
            before: policy(Mode::All, &[]),
            after: policy(Mode::Denylist, &["claude-one"]),
            group_ids: vec!["g1".into()],
            manual_models: vec![],
            expected_group_version: None,
        };
        assert!(resolve_policy(&req, &doc).is_ok());
        req.after.models.push("other".into());
        assert!(resolve_policy(&req, &doc).is_err());
    }
    #[test]
    fn equivalence_ignores_model_order_not_mode() {
        assert!(
            policy(Mode::Denylist, &["a", "b"]).equivalent(&policy(Mode::Denylist, &["b", "a"]))
        );
        assert!(!policy(Mode::Allowlist, &["a"]).equivalent(&policy(Mode::Denylist, &["a"])));
    }
}

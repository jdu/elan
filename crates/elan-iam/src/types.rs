use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subject {
    pub user_id: String,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceId {
    pub namespace: String,
    pub name: String,
}

impl ResourceId {
    pub fn qualified(&self) -> String {
        format!("{}.{}", self.namespace, self.name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Allow,
    Deny,
}

#[derive(Debug, Clone)]
pub enum MaskKind {
    Redact,
    Sha256,
}

#[derive(Debug, Clone)]
pub struct ColumnMask(pub HashMap<String, MaskKind>);

#[derive(Debug, Clone)]
pub enum AccessDecision {
    Allow {
        row_filter: Option<String>,
        column_mask: Option<ColumnMask>,
    },
    Deny {
        reason: String,
    },
}

impl AccessDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, AccessDecision::Allow { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub id: Uuid,
    pub subject_name: String,
    pub subject_type: SubjectType,
    pub resource_pattern: String,
    pub action: String,
    pub effect: PolicyEffect,
    pub row_filter: Option<String>,
    pub column_mask_json: Option<String>,
    pub priority: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PolicyEffect {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubjectType {
    User,
    Group,
}

impl Policy {
    pub fn matches_resource(&self, namespace: &str, name: &str) -> bool {
        let pattern = &self.resource_pattern;
        if pattern == "*" {
            return true;
        }
        // "finance.*" matches any dataset in namespace "finance"
        if let Some(ns_pattern) = pattern.strip_suffix(".*") {
            return ns_pattern == namespace;
        }
        // exact match "finance.transactions"
        pattern == &format!("{namespace}.{name}")
    }

    pub fn matches_action(&self, action: &str) -> bool {
        self.action == "*" || self.action.eq_ignore_ascii_case(action)
    }

    pub fn matches_subject(&self, subject: &Subject) -> bool {
        match self.subject_type {
            SubjectType::User => {
                self.subject_name == "*" || self.subject_name == subject.user_id
            }
            SubjectType::Group => subject.groups.contains(&self.subject_name),
        }
    }
}

use crate::types::{AccessDecision, ColumnMask, MaskKind, Policy, PolicyEffect, ResourceId, Subject};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::debug;

pub trait IamEngine: Send + Sync + 'static + std::fmt::Debug {
    fn check(&self, subject: &Subject, resource: &ResourceId, action: &str) -> AccessDecision;
    fn reload(&self, policies: Vec<Policy>);
}

/// In-memory policy engine backed by a snapshot loaded from elan-central.
/// Refreshed by calling `reload()` periodically.
#[derive(Debug)]
pub struct SnapshotIamEngine {
    policies: Arc<RwLock<Vec<Policy>>>,
}

impl SnapshotIamEngine {
    pub fn new(policies: Vec<Policy>) -> Arc<Self> {
        Arc::new(Self {
            policies: Arc::new(RwLock::new(policies)),
        })
    }
}

impl IamEngine for SnapshotIamEngine {
    fn check(&self, subject: &Subject, resource: &ResourceId, action: &str) -> AccessDecision {
        let policies = self.policies.read().unwrap();

        // Collect matching policies, sorted by priority desc then Deny-first
        let mut matching: Vec<&Policy> = policies
            .iter()
            .filter(|p| {
                p.matches_subject(subject)
                    && p.matches_resource(&resource.namespace, &resource.name)
                    && p.matches_action(action)
            })
            .collect();

        // Explicit Deny takes precedence over Allow; within same effect, higher priority wins
        matching.sort_by(|a, b| {
            // Deny < Allow for sort order (we want Deny first)
            let effect_ord = |e: &PolicyEffect| match e {
                PolicyEffect::Deny => 0,
                PolicyEffect::Allow => 1,
            };
            effect_ord(&a.effect)
                .cmp(&effect_ord(&b.effect))
                .then(b.priority.cmp(&a.priority))
        });

        if let Some(policy) = matching.first() {
            debug!(
                user = %subject.user_id,
                resource = %resource.qualified(),
                effect = ?policy.effect,
                policy_id = %policy.id,
                "IAM decision"
            );

            match policy.effect {
                PolicyEffect::Deny => AccessDecision::Deny {
                    reason: format!(
                        "Deny policy '{}' matched for {}.{}",
                        policy.id, resource.namespace, resource.name
                    ),
                },
                PolicyEffect::Allow => {
                    let column_mask = policy.column_mask_json.as_deref().and_then(|json| {
                        serde_json::from_str::<HashMap<String, String>>(json)
                            .ok()
                            .map(|m| {
                                ColumnMask(
                                    m.into_iter()
                                        .filter_map(|(k, v)| {
                                            let kind = match v.as_str() {
                                                "REDACT" => Some(MaskKind::Redact),
                                                "SHA256" => Some(MaskKind::Sha256),
                                                _ => None,
                                            };
                                            kind.map(|k2| (k, k2))
                                        })
                                        .collect(),
                                )
                            })
                    });

                    AccessDecision::Allow {
                        row_filter: policy.row_filter.clone(),
                        column_mask,
                    }
                }
            }
        } else {
            // Default deny — no matching policy means no access
            AccessDecision::Deny {
                reason: format!(
                    "No Allow policy for {} on {}.{}",
                    subject.user_id, resource.namespace, resource.name
                ),
            }
        }
    }

    fn reload(&self, policies: Vec<Policy>) {
        let mut guard = self.policies.write().unwrap();
        *guard = policies;
    }
}

/// Permissive engine for use in tests or when IAM is disabled.
#[derive(Debug)]
pub struct AllowAllEngine;

impl IamEngine for AllowAllEngine {
    fn check(&self, _subject: &Subject, _resource: &ResourceId, _action: &str) -> AccessDecision {
        AccessDecision::Allow {
            row_filter: None,
            column_mask: None,
        }
    }

    fn reload(&self, _policies: Vec<Policy>) {}
}

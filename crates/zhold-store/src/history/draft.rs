use zhold_core::HistoryKind;

use super::{
    BuildReceipt, CollectionReceipt, CollectionReceiptSource, HistoryDraft, HistoryPayload,
    HookReceipt, QuotaReceipt, QuotaReceiptAction,
};
use crate::{CollectionReport, HookReport, QuotaStatus, WorktreeIntegration};

impl HistoryDraft {
    pub(crate) fn build(receipt: BuildReceipt, integration: Option<&WorktreeIntegration>) -> Self {
        let mut receipt = receipt;
        if let Some(integration) = integration {
            receipt.manager.clone_from(&integration.manager);
            receipt.label.clone_from(&integration.label);
            receipt.session.clone_from(&integration.session);
        }
        Self {
            kind: HistoryKind::Build,
            payload: HistoryPayload::Build(receipt),
        }
    }

    pub(crate) fn collection(report: &CollectionReport, source: CollectionReceiptSource) -> Self {
        Self {
            kind: HistoryKind::Collection,
            payload: HistoryPayload::Collection(CollectionReceipt {
                source,
                budget: report.budget,
                reserved: report.reserved,
                before: report.plan.before,
                target: report.plan.target,
                projected: report.plan.projected,
                after: report.after,
                protected: report.plan.protected,
                reclaimed: report.reclaimed,
                budget_met: report.budget_met,
                retirements: report
                    .retirements
                    .iter()
                    .map(|retirement| retirement.arena_id.clone())
                    .collect(),
                skipped: report
                    .skipped
                    .iter()
                    .map(|skipped| skipped.arena_id.clone())
                    .collect(),
            }),
        }
    }

    pub(crate) fn trash(report: &crate::TrashReport) -> Self {
        Self {
            kind: HistoryKind::Collection,
            payload: HistoryPayload::Collection(CollectionReceipt {
                source: CollectionReceiptSource::TrashRetry,
                budget: zhold_core::ByteSize::ZERO,
                reserved: zhold_core::ByteSize::ZERO,
                before: report.before,
                target: zhold_core::ByteSize::ZERO,
                projected: report.remaining,
                after: report.remaining,
                protected: zhold_core::ByteSize::ZERO,
                reclaimed: report.reclaimed,
                budget_met: report.remaining == zhold_core::ByteSize::ZERO,
                retirements: Vec::new(),
                skipped: Vec::new(),
            }),
        }
    }

    pub(crate) fn hook(report: &HookReport) -> Option<Self> {
        let integration = report.integration.as_ref()?;
        Some(Self {
            kind: HistoryKind::Hook,
            payload: HistoryPayload::Hook(HookReceipt {
                event: report.event,
                repository_id: integration.repository_id.clone(),
                worktree_id: integration.worktree_id.clone(),
                manager: integration.manager.clone(),
                label: integration.label.clone(),
                session: integration.session.clone(),
                previous: report.previous,
                resulting: integration.state,
                result: report.result,
            }),
        })
    }

    pub(crate) fn quota(status: &QuotaStatus, action: QuotaReceiptAction) -> Option<Self> {
        let observation = &status.observation;
        Some(Self {
            kind: HistoryKind::Quota,
            payload: HistoryPayload::Quota(QuotaReceipt {
                provider: observation.provider,
                scope: observation.scope.clone(),
                filesystem_id: observation
                    .filesystem_id
                    .clone()
                    .filter(|value| crate::quota::valid_identity(value))?,
                expected_limit: status.expectation.as_ref().map(|value| value.hard_limit),
                observed_limit: observation.limit,
                observed_usage: observation.usage,
                action,
                result: observation.health,
            }),
        })
    }
}

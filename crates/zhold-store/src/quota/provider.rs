use std::path::Path;

use zhold_core::QuotaProvider;

use super::QuotaObservation;

pub(crate) trait QuotaProbe {
    fn inspect(&self, root: &Path, requested: QuotaProvider) -> QuotaObservation;
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SystemQuotaProbe;

impl QuotaProbe for SystemQuotaProbe {
    fn inspect(&self, root: &Path, requested: QuotaProvider) -> QuotaObservation {
        super::inspect(root, requested)
    }
}

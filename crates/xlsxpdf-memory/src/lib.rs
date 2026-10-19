use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Subsystem {
    ZipXml,
    Sst,
    TextShaping,
    FontRuntime,
    LayoutArena,
    ImageProcessing,
    PdfPageArena,
    MetadataIndexes,
    RuntimeHeadroom,
}

impl Subsystem {
    pub fn default_budget_256mb(self) -> u64 {
        match self {
            Self::ZipXml => 12 << 20,
            Self::Sst => 24 << 20,
            Self::TextShaping => 24 << 20,
            Self::FontRuntime => 20 << 20,
            Self::LayoutArena => 24 << 20,
            Self::ImageProcessing => 32 << 20,
            Self::PdfPageArena => 24 << 20,
            Self::MetadataIndexes => 16 << 20,
            Self::RuntimeHeadroom => 70 << 20,
        }
    }
}

const SUBSYSTEM_COUNT: usize = 9;

#[derive(Debug)]
struct SubsystemState {
    budget: AtomicU64,
    used: AtomicU64,
}

#[derive(Debug)]
pub struct MemoryGovernor {
    total_limit: u64,
    subsystems: [SubsystemState; SUBSYSTEM_COUNT],
}

impl MemoryGovernor {
    pub fn new(total_limit: u64) -> Arc<Self> {
        let subsystems = std::array::from_fn(|_| SubsystemState {
            budget: AtomicU64::new(0),
            used: AtomicU64::new(0),
        });

        let gov = Arc::new(Self {
            total_limit,
            subsystems,
        });

        let all_subsystems = [
            Subsystem::ZipXml,
            Subsystem::Sst,
            Subsystem::TextShaping,
            Subsystem::FontRuntime,
            Subsystem::LayoutArena,
            Subsystem::ImageProcessing,
            Subsystem::PdfPageArena,
            Subsystem::MetadataIndexes,
            Subsystem::RuntimeHeadroom,
        ];

        let default_total: u64 = all_subsystems
            .iter()
            .map(|s| s.default_budget_256mb())
            .sum();
        let scale = total_limit as f64 / default_total as f64;

        for (i, sub) in all_subsystems.iter().enumerate() {
            let budget = (sub.default_budget_256mb() as f64 * scale) as u64;
            gov.subsystems[i]
                .budget
                .store(budget, Ordering::Relaxed);
        }

        gov
    }

    pub fn try_allocate(&self, subsystem: Subsystem, bytes: u64) -> Result<(), MemoryError> {
        let idx = subsystem as usize;
        let state = &self.subsystems[idx];
        let budget = state.budget.load(Ordering::Relaxed);
        let prev = state.used.fetch_add(bytes, Ordering::Relaxed);
        if prev + bytes > budget {
            state.used.fetch_sub(bytes, Ordering::Relaxed);
            return Err(MemoryError::BudgetExceeded {
                subsystem,
                budget,
                requested: bytes,
                used: prev,
            });
        }
        Ok(())
    }

    pub fn release(&self, subsystem: Subsystem, bytes: u64) {
        let idx = subsystem as usize;
        self.subsystems[idx]
            .used
            .fetch_sub(bytes, Ordering::Relaxed);
    }

    pub fn usage(&self, subsystem: Subsystem) -> (u64, u64) {
        let idx = subsystem as usize;
        let state = &self.subsystems[idx];
        (
            state.used.load(Ordering::Relaxed),
            state.budget.load(Ordering::Relaxed),
        )
    }

    pub fn total_limit(&self) -> u64 {
        self.total_limit
    }

    pub fn total_used(&self) -> u64 {
        self.subsystems
            .iter()
            .map(|s| s.used.load(Ordering::Relaxed))
            .sum()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("memory budget exceeded for {subsystem:?}: budget={budget}, used={used}, requested={requested}")]
    BudgetExceeded {
        subsystem: Subsystem,
        budget: u64,
        used: u64,
        requested: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_allocate_release() {
        let gov = MemoryGovernor::new(256 << 20);
        gov.try_allocate(Subsystem::Sst, 1024).unwrap();
        let (used, budget) = gov.usage(Subsystem::Sst);
        assert_eq!(used, 1024);
        assert!(budget > 0);
        gov.release(Subsystem::Sst, 1024);
        let (used, _) = gov.usage(Subsystem::Sst);
        assert_eq!(used, 0);
    }

    #[test]
    fn test_budget_exceeded() {
        let gov = MemoryGovernor::new(1 << 20); // 1 MB total
        let result = gov.try_allocate(Subsystem::Sst, 2 << 20); // request 2 MB
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_allocations() {
        let gov = MemoryGovernor::new(256 << 20);
        for _ in 0..100 {
            gov.try_allocate(Subsystem::LayoutArena, 1024).unwrap();
        }
        let (used, _) = gov.usage(Subsystem::LayoutArena);
        assert_eq!(used, 100 * 1024);
        for _ in 0..100 {
            gov.release(Subsystem::LayoutArena, 1024);
        }
        let (used, _) = gov.usage(Subsystem::LayoutArena);
        assert_eq!(used, 0);
    }

    #[test]
    fn test_total_used() {
        let gov = MemoryGovernor::new(256 << 20);
        gov.try_allocate(Subsystem::Sst, 1000).unwrap();
        gov.try_allocate(Subsystem::LayoutArena, 2000).unwrap();
        assert_eq!(gov.total_used(), 3000);
        gov.release(Subsystem::Sst, 1000);
        assert_eq!(gov.total_used(), 2000);
    }

    #[test]
    fn test_budget_scaling() {
        let gov_small = MemoryGovernor::new(64 << 20); // 64 MB
        let gov_large = MemoryGovernor::new(512 << 20); // 512 MB
        let (_, budget_small) = gov_small.usage(Subsystem::Sst);
        let (_, budget_large) = gov_large.usage(Subsystem::Sst);
        assert!(budget_large > budget_small);
        let ratio = budget_large as f64 / budget_small as f64;
        assert!((ratio - 8.0).abs() < 0.5);
    }

    #[test]
    fn test_exhaust_and_recover() {
        let gov = MemoryGovernor::new(1 << 20);
        let (_, budget) = gov.usage(Subsystem::ImageProcessing);
        gov.try_allocate(Subsystem::ImageProcessing, budget).unwrap();
        assert!(gov.try_allocate(Subsystem::ImageProcessing, 1).is_err());
        gov.release(Subsystem::ImageProcessing, budget);
        gov.try_allocate(Subsystem::ImageProcessing, 1024).unwrap();
    }

    #[test]
    fn test_zero_allocation() {
        let gov = MemoryGovernor::new(256 << 20);
        gov.try_allocate(Subsystem::Sst, 0).unwrap();
    }

    #[test]
    fn test_all_subsystems_have_budget() {
        let gov = MemoryGovernor::new(256 << 20);
        let subsystems = [
            Subsystem::ZipXml, Subsystem::Sst, Subsystem::TextShaping,
            Subsystem::FontRuntime, Subsystem::LayoutArena, Subsystem::ImageProcessing,
            Subsystem::PdfPageArena, Subsystem::MetadataIndexes, Subsystem::RuntimeHeadroom,
        ];
        for sub in &subsystems {
            let (_, budget) = gov.usage(*sub);
            assert!(budget > 0, "{sub:?} has no budget");
        }
    }
}

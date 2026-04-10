/*!
# cuda-contract

Agent contracts and SLAs.

Agents make promises to each other — response times, reliability
guarantees, resource commitments. This crate formalizes those
agreements and tracks compliance.

- Service Level Agreements (SLAs)
- Capability guarantees
- Quality of Service tiers
- Contract negotiation
- Compliance tracking
- Penalty/reward calculation
*/

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// QoS tier
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QosTier { BestEffort = 0, Standard = 1, Premium = 2, Guaranteed = 3 }

/// SLA metric type
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlaMetric { ResponseTimeMs, AvailabilityPct, ThroughputPerSec, ErrorRatePct }

/// An SLA clause
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlaClause {
    pub metric: SlaMetric,
    pub target: f64,
    pub minimum: f64,     // below this = violation
    pub penalty_per_violation: f64,
    pub window_ms: u64,
}

/// An agent contract
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Contract {
    pub id: String,
    pub provider: String,
    pub consumer: String,
    pub capabilities: Vec<String>,
    pub qos_tier: QosTier,
    pub clauses: Vec<SlaClause>,
    pub created: u64,
    pub expires_ms: Option<u64>,
    pub active: bool,
}

impl Contract {
    pub fn is_expired(&self) -> bool {
        self.expires_ms.map_or(false, |e| now() > e)
    }
}

/// Compliance record
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComplianceRecord {
    pub contract_id: String,
    pub clause_index: usize,
    pub metric_value: f64,
    pub compliant: bool,
    pub timestamp: u64,
    pub penalty: f64,
}

/// Contract negotiation proposal
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub capabilities: Vec<String>,
    pub qos_requested: QosTier,
    pub clauses: Vec<SlaClause>,
    pub max_penalty: f64,
}

/// The contract manager
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractManager {
    pub contracts: HashMap<String, Contract>,
    pub compliance: Vec<ComplianceRecord>,
    pub penalties: HashMap<String, f64>,
    pub total_contracts: u64,
    pub active_contracts: u64,
}

impl ContractManager {
    pub fn new() -> Self { ContractManager { contracts: HashMap::new(), compliance: vec![], penalties: HashMap::new(), total_contracts: 0, active_contracts: 0 } }

    /// Create a contract
    pub fn create(&mut self, provider: &str, consumer: &str, capabilities: &[&str], qos: QosTier, clauses: Vec<SlaClause>) -> String {
        let id = format!("ctr_{}", self.total_contracts + 1);
        let contract = Contract { id: id.clone(), provider: provider.to_string(), consumer: consumer.to_string(), capabilities: capabilities.iter().map(|s| s.to_string()).collect(), qos_tier: qos, clauses, created: now(), expires_ms: None, active: true };
        self.total_contracts += 1;
        self.active_contracts += 1;
        self.contracts.insert(id.clone(), contract);
        id
    }

    /// Record compliance measurement
    pub fn record_compliance(&mut self, contract_id: &str, clause_idx: usize, value: f64) -> ComplianceRecord {
        let contract = self.contracts.get(contract_id);
        let clause = contract.map(|c| c.clauses.get(clause_idx));
        let (compliant, penalty) = match clause {
            Some(cl) => (value >= cl.minimum, if value < cl.minimum { cl.penalty_per_violation } else { 0.0 }),
            None => (true, 0.0),
        };
        let record = ComplianceRecord { contract_id: contract_id.to_string(), clause_index: clause_idx, metric_value: value, compliant, timestamp: now(), penalty };
        if penalty > 0.0 {
            *self.penalties.entry(contract_id.to_string()).or_insert(0.0) += penalty;
        }
        self.compliance.push(record.clone());
        record
    }

    /// Check overall contract health
    pub fn contract_health(&self, contract_id: &str) -> f64 {
        let records: Vec<&ComplianceRecord> = self.compliance.iter().filter(|r| r.contract_id == contract_id).collect();
        if records.is_empty() { return 1.0; }
        let compliant = records.iter().filter(|r| r.compliant).count();
        compliant as f64 / records.len() as f64
    }

    /// Get total penalties for a contract
    pub fn total_penalty(&self, contract_id: &str) -> f64 {
        self.penalties.get(contract_id).copied().unwrap_or(0.0)
    }

    /// Deactivate expired contracts
    pub fn expire_contracts(&mut self) -> usize {
        let mut expired = 0;
        for contract in self.contracts.values_mut() {
            if contract.active && contract.is_expired() {
                contract.active = false;
                expired += 1;
                self.active_contracts -= 1;
            }
        }
        expired
    }

    /// Find contracts for a provider
    pub fn contracts_for(&self, agent_id: &str) -> Vec<&Contract> {
        self.contracts.values().filter(|c| c.provider == agent_id && c.active).collect()
    }

    /// Summary
    pub fn summary(&self) -> String {
        let total_penalties: f64 = self.penalties.values().sum();
        format!("Contracts: {}/{} active, {} compliance records, total penalties={:.2}",
            self.active_contracts, self.total_contracts, self.compliance.len(), total_penalties)
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_contract() {
        let mut cm = ContractManager::new();
        let id = cm.create("provider", "consumer", &["nav", "sense"], QosTier::Standard, vec![]);
        assert!(cm.contracts.contains_key(&id));
    }

    #[test]
    fn test_compliance_pass() {
        let mut cm = ContractManager::new();
        let clause = SlaClause { metric: SlaMetric::ResponseTimeMs, target: 100.0, minimum: 50.0, penalty_per_violation: 1.0, window_ms: 1000 };
        let id = cm.create("p", "c", &["x"], QosTier::Standard, vec![clause]);
        let record = cm.record_compliance(&id, 0, 80.0);
        assert!(record.compliant);
    }

    #[test]
    fn test_compliance_fail_with_penalty() {
        let mut cm = ContractManager::new();
        let clause = SlaClause { metric: SlaMetric::ResponseTimeMs, target: 100.0, minimum: 50.0, penalty_per_violation: 5.0, window_ms: 1000 };
        let id = cm.create("p", "c", &["x"], QosTier::Standard, vec![clause]);
        let record = cm.record_compliance(&id, 0, 30.0); // below minimum
        assert!(!record.compliant);
        assert_eq!(cm.total_penalty(&id), 5.0);
    }

    #[test]
    fn test_contract_health() {
        let mut cm = ContractManager::new();
        let clause = SlaClause { metric: SlaMetric::AvailabilityPct, target: 99.9, minimum: 95.0, penalty_per_violation: 1.0, window_ms: 60000 };
        let id = cm.create("p", "c", &["x"], QosTier::Standard, vec![clause]);
        cm.record_compliance(&id, 0, 99.0); // pass
        cm.record_compliance(&id, 0, 96.0); // pass
        cm.record_compliance(&id, 0, 90.0); // fail
        let health = cm.contract_health(&id);
        assert!((health - 0.6667).abs() < 0.01);
    }

    #[test]
    fn test_contracts_for_provider() {
        let mut cm = ContractManager::new();
        cm.create("p1", "c1", &["x"], QosTier::Standard, vec![]);
        cm.create("p1", "c2", &["y"], QosTier::Premium, vec![]);
        cm.create("p2", "c3", &["z"], QosTier::Standard, vec![]);
        assert_eq!(cm.contracts_for("p1").len(), 2);
    }

    #[test]
    fn test_expire_contracts() {
        let mut cm = ContractManager::new();
        let id = cm.create("p", "c", &["x"], QosTier::Standard, vec![]);
        if let Some(c) = cm.contracts.get_mut(&id) { c.expires_ms = Some(0); }
        let expired = cm.expire_contracts();
        assert_eq!(expired, 1);
    }

    #[test]
    fn test_qos_tier_ordering() {
        assert!(QosTier::Guaranteed > QosTier::Premium);
        assert!(QosTier::Premium > QosTier::Standard);
        assert!(QosTier::Standard > QosTier::BestEffort);
    }

    #[test]
    fn test_summary() {
        let cm = ContractManager::new();
        let s = cm.summary();
        assert!(s.contains("0/0 active"));
    }
}

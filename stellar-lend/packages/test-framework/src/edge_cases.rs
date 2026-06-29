use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeCase {
    pub id: String,
    pub function: String,
    pub description: String,
    pub input: String,
    pub expected_behavior: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeCaseCatalog {
    pub cases: Vec<EdgeCase>,
}

impl EdgeCaseCatalog {
    pub fn new() -> Self {
        EdgeCaseCatalog {
            cases: Vec::new(),
        }
    }

    pub fn add_case(&mut self, case: EdgeCase) {
        self.cases.push(case);
    }

    pub fn get_cases_for_function(&self, function: &str) -> Vec<&EdgeCase> {
        self.cases
            .iter()
            .filter(|c| c.function == function)
            .collect()
    }
}

pub fn deposit_edge_cases() -> Vec<EdgeCase> {
    vec![
        EdgeCase {
            id: "deposit_001".to_string(),
            function: "deposit".to_string(),
            description: "Deposit with zero amount".to_string(),
            input: "amount: 0".to_string(),
            expected_behavior: "Should revert with InvalidAmount".to_string(),
        },
        EdgeCase {
            id: "deposit_002".to_string(),
            function: "deposit".to_string(),
            description: "Deposit when paused".to_string(),
            input: "normal deposit, pool paused".to_string(),
            expected_behavior: "Should revert with PoolPaused".to_string(),
        },
        EdgeCase {
            id: "deposit_003".to_string(),
            function: "deposit".to_string(),
            description: "Deposit at maximum supply limit".to_string(),
            input: "amount at supply cap".to_string(),
            expected_behavior: "Should revert with SupplyCapReached".to_string(),
        },
    ]
}

pub fn liquidation_edge_cases() -> Vec<EdgeCase> {
    vec![
        EdgeCase {
            id: "liquidate_001".to_string(),
            function: "liquidate".to_string(),
            description: "Liquidate healthy position".to_string(),
            input: "borrower with health factor > 1.0".to_string(),
            expected_behavior: "Should revert with PositionHealthy".to_string(),
        },
        EdgeCase {
            id: "liquidate_002".to_string(),
            function: "liquidate".to_string(),
            description: "Liquidation with stale oracle".to_string(),
            input: "oracle price older than threshold".to_string(),
            expected_behavior: "Should revert with StaleOracle".to_string(),
        },
        EdgeCase {
            id: "liquidate_003".to_string(),
            function: "liquidate".to_string(),
            description: "Unprofitable liquidation".to_string(),
            input: "gas cost > liquidation bonus".to_string(),
            expected_behavior: "Should revert with Unprofitable".to_string(),
        },
    ]
}

pub fn oracle_edge_cases() -> Vec<EdgeCase> {
    vec![
        EdgeCase {
            id: "oracle_001".to_string(),
            function: "getPrice".to_string(),
            description: "All feeds stale".to_string(),
            input: "all oracle feeds older than threshold".to_string(),
            expected_behavior: "Should revert or return sentinel value".to_string(),
        },
        EdgeCase {
            id: "oracle_002".to_string(),
            function: "getPrice".to_string(),
            description: "Feed removal with active positions".to_string(),
            input: "remove primary feed while positions depend on it".to_string(),
            expected_behavior: "Should fall back to secondary feed".to_string(),
        },
    ]
}

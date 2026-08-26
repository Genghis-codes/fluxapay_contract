use crate::{FXOracle, FXOracleClient, FXOracleError};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, Symbol,
};

fn setup_oracle(env: &Env) -> (Address, FXOracleClient<'_>) {
    let contract_id = env.register(FXOracle, ());
    let client = FXOracleClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.oracle_initialize(&admin, &86400); // 24 hour threshold
    (admin, client)
}

#[test]
fn test_set_and_get_rate() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_oracle(&env);

    let oracle = Address::generate(&env);
    client.oracle_grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);

    let pair = Symbol::new(&env, "USDC_NGN");
    let rate = 1500_0000000i128; // 1500 NGN/USDC
    let decimals = 7;

    client.set_rate(&oracle, &pair, &rate, &decimals);

    let rate_data = client.get_rate(&pair);
    assert_eq!(rate_data.rate, rate);
    assert_eq!(rate_data.decimals, decimals);
    assert_eq!(rate_data.pair, pair);
    assert_eq!(rate_data.updated_at, env.ledger().timestamp());
}

#[test]
fn test_unauthorized_set_rate() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client) = setup_oracle(&env);

    let unauthorized_user = Address::generate(&env);
    let pair = Symbol::new(&env, "USDC_NGN");

    let result = client.try_set_rate(&unauthorized_user, &pair, &1000i128, &2);
    assert_eq!(result, Err(Ok(FXOracleError::Unauthorized)));
}

#[test]
fn test_staleness_check() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_oracle(&env);

    let oracle = Address::generate(&env);
    client.oracle_grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);

    let pair = Symbol::new(&env, "USDC_NGN");
    client.set_rate(&oracle, &pair, &1500i128, &0);

    // Jump forward 25 hours (threshold is 24)
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 25 * 3600);

    let result = client.try_get_rate(&pair);
    assert_eq!(result, Err(Ok(FXOracleError::RateStale)));
}

#[test]
fn test_hard_staleness_cap_despite_high_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_oracle(&env);

    let oracle = Address::generate(&env);
    client.oracle_grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);

    // Admin sets a permissive 7-day threshold; hard cap still applies at 24 h.
    client.set_staleness_threshold(&admin, &(7 * 86_400));

    let pair = Symbol::new(&env, "USDC_NGN");
    client.set_rate(&oracle, &pair, &1500i128, &0);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 25 * 3600);

    let result = client.try_get_rate(&pair);
    assert_eq!(result, Err(Ok(FXOracleError::RateStale)));
}

#[test]
fn test_circuit_breaker_rejects_rate_by_ledger_gap() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_oracle(&env);

    let oracle = Address::generate(&env);
    client.oracle_grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);

    let pair = Symbol::new(&env, "USDC_NGN");
    client.set_rate(&oracle, &pair, &1500i128, &0);

    let seq_at_update = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq_at_update + 17_281);

    let result = client.try_get_rate(&pair);
    assert_eq!(result, Err(Ok(FXOracleError::RateStale)));
}

#[test]
fn test_settlement_amount_calculation() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_oracle(&env);

    let oracle = Address::generate(&env);
    client.oracle_grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);

    // 1 USDC = 1500.50 NGN (2 decimals: 150050)
    let pair = Symbol::new(&env, "NGN");
    client.set_rate(&oracle, &pair, &150050i128, &2);

    // 100 USDC -> 150050 NGN
    let usdc_amount = 100i128;
    let expected_fiat = 150050i128; // (100 * 150050) / 100

    let amount = client.get_settlement_amount(&usdc_amount, &pair);
    assert_eq!(amount, expected_fiat);
}

#[test]
fn test_update_staleness_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_oracle(&env);

    client.set_staleness_threshold(&admin, &3600);
    assert_eq!(client.get_staleness_threshold(), 3600);
}

#[test]
fn test_oracle_grant_role_by_admin_grants_role() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_oracle(&env);
    let oracle = Address::generate(&env);
    let role = Symbol::new(&env, "ORACLE");

    client.oracle_grant_role(&admin, &role, &oracle);
    assert!(client.oracle_has_role(&role, &oracle));
}

#[test]
fn test_oracle_grant_role_by_non_admin_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client) = setup_oracle(&env);
    let non_admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let role = Symbol::new(&env, "ORACLE");

    let result = client.try_oracle_grant_role(&non_admin, &role, &oracle);
    assert_eq!(result, Err(Ok(FXOracleError::Unauthorized)));
}

#[test]
fn test_get_fx_admin_returns_initialized_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_oracle(&env);

    assert_eq!(client.get_fx_admin(), Some(admin));
}

#[test]
fn test_get_fx_admin_before_initialization_returns_none() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(FXOracle, ());
    let client = FXOracleClient::new(&env, &contract_id);

    assert_eq!(client.get_fx_admin(), None);
}

#[test]
fn test_check_rate_staleness_emits_alert() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_oracle(&env);

    let oracle = Address::generate(&env);
    client.oracle_grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);

    let pair = Symbol::new(&env, "USDC_NGN");
    client.set_rate(&oracle, &pair, &1500i128, &0);

    assert!(!client.check_rate_staleness(&pair));

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 25 * 3600);

    // Stale → returns true (and emits RATE/STALE_ALERT on-chain).
    assert!(client.check_rate_staleness(&pair));
}

#[test]
fn test_set_rates_batch_stores_all_rates() {
    use soroban_sdk::vec;

    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_oracle(&env);

    let oracle = Address::generate(&env);
    client.oracle_grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);

    let rates = vec![
        &env,
        (Symbol::new(&env, "USD"), 1_0000000i128, 7u32),
        (Symbol::new(&env, "NGN"), 1500_0000000i128, 7u32),
        (Symbol::new(&env, "EUR"), 9200000i128, 7u32),
    ];

    let count = client.set_rates_batch(&oracle, &rates);
    assert_eq!(count, 3);

    assert_eq!(client.get_rate(&Symbol::new(&env, "USD")).rate, 1_0000000i128);
    assert_eq!(client.get_rate(&Symbol::new(&env, "NGN")).rate, 1500_0000000i128);
    assert_eq!(client.get_rate(&Symbol::new(&env, "EUR")).rate, 9200000i128);
}

#[test]
fn test_set_rates_batch_rejects_non_oracle() {
    use soroban_sdk::vec;

    let env = Env::default();
    env.mock_all_auths();
    let (_admin, client) = setup_oracle(&env);

    let unauthorized = Address::generate(&env);
    let rates = vec![&env, (Symbol::new(&env, "USD"), 1i128, 0u32)];

    let result = client.try_set_rates_batch(&unauthorized, &rates);
    assert_eq!(result, Err(Ok(FXOracleError::Unauthorized)));
}

#[test]
fn test_set_rates_batch_rejects_oversized_batch() {
    use soroban_sdk::vec;

    let env = Env::default();
    env.mock_all_auths();
    let (admin, client) = setup_oracle(&env);

    let oracle = Address::generate(&env);
    client.oracle_grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);

    let mut rates = vec![&env];
    for _ in 0..21u32 {
        rates.push_back((Symbol::new(&env, "USD"), 1i128, 0u32));
    }

    let result = client.try_set_rates_batch(&oracle, &rates);
    assert_eq!(result, Err(Ok(FXOracleError::BatchTooLarge)));
}

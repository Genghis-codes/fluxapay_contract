use crate::{
    FiatConfig, LinkAnalytics, MaybeFiatConfig, PaymentLinkManager, PaymentLinkManagerClient, FXOracle,
    FXOracleClient,
};


use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, vec, Address, BytesN, Env, Map, String, Symbol,
};

fn setup_payment_link(env: &Env) -> (Address, PaymentLinkManagerClient<'_>) {
    let contract_id = env.register(PaymentLinkManager, ());
    let client = PaymentLinkManagerClient::new(env, &contract_id);
    let admin = Address::generate(env);
    (admin, client)
}

#[test]
fn test_create_link() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);

    let link_id = String::from_str(&env, "link_123");
    let amount = Some(1000i128);
    let currency = Symbol::new(&env, "USDC");
    let description = String::from_str(&env, "Test Link");

    let id = client.create_link(
        &merchant,
        &link_id,
        &amount,
        &currency,
        &description,
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    assert_eq!(id, link_id);
    let link = client.get_link(&link_id);
    assert_eq!(link.merchant_id, merchant);
    assert_eq!(link.amount, amount);
    assert!(link.active);
    assert!(!link.direct_transfer);
}

#[test]
fn test_use_link_fixed_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "fixed_link");
    let amount = 1000i128;
    client.create_link(
        &merchant,
        &link_id,
        &Some(amount),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Fixed"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    let payment_id = client.use_link(&payer, &link_id, &amount, &None);
    assert!(!payment_id.is_empty());

    let link = client.get_link(&link_id);
    assert_eq!(link.use_count, 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #406)")]
fn test_use_link_wrong_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "fixed_link_wrong");
    client.create_link(
        &merchant,
        &link_id,
        &Some(1000i128),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Fixed"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    client.use_link(&payer, &link_id, &500i128, &None);
}

#[test]
fn test_use_link_open_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "open_link");
    client.create_link(
        &merchant,
        &link_id,
        &None,
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Open"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    client.use_link(&payer, &link_id, &1500i128, &None);
    let link = client.get_link(&link_id);
    assert_eq!(link.use_count, 1);
}

#[test]
fn test_deactivate_link() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);

    let link_id = String::from_str(&env, "deactivate_me");
    client.create_link(
        &merchant,
        &link_id,
        &None,
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Bye"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    client.deactivate_link(&merchant, &link_id);
    let link = client.get_link(&link_id);
    assert!(!link.active);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_link_expired() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "expired_link");
    let expiry = 1000u64;
    client.create_link(
        &merchant,
        &link_id,
        &None,
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Old"),
        &Some(expiry),
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    env.ledger().set_timestamp(expiry + 1);
    client.use_link(&payer, &link_id, &100i128, &None);
}

#[test]
fn test_verify_batch_returns_status_for_active_links() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);

    let link_id1 = String::from_str(&env, "batch_link_1");
    let link_id2 = String::from_str(&env, "batch_link_2");

    client.create_link(
        &merchant,
        &link_id1,
        &Some(500i128),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Batch 1"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );
    client.create_link(
        &merchant,
        &link_id2,
        &Some(1000i128),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Batch 2"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    let results = client.verify_batch(&vec![&env, link_id1.clone(), link_id2.clone()]);
    assert_eq!(results.len(), 2);
    assert_eq!(results.get(0).unwrap(), (link_id1.clone(), true, 0, None));
    assert_eq!(results.get(1).unwrap(), (link_id2.clone(), true, 0, None));
}

#[test]
fn test_verify_batch_handles_missing_links() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);

    let existing_link = String::from_str(&env, "existing_batch_link");
    let missing_link = String::from_str(&env, "missing_batch_link");

    client.create_link(
        &merchant,
        &existing_link,
        &Some(1000i128),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Existing"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    let results = client.verify_batch(&vec![&env, existing_link.clone(), missing_link.clone()]);
    assert_eq!(results.len(), 2);
    assert_eq!(results.get(0).unwrap(), (existing_link.clone(), true, 0, None));
    assert_eq!(results.get(1).unwrap(), (missing_link.clone(), false, 0, None));
}

#[test]
fn test_verify_batch_returns_inactive_for_deactivated_link() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);

    let link_id = String::from_str(&env, "deactivated_batch_link");
    client.create_link(
        &merchant,
        &link_id,
        &Some(1000i128),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Deactivated"),
        &None,
        &Some(10),
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    client.deactivate_link(&merchant, &link_id);

    let results = client.verify_batch(&vec![&env, link_id.clone()]);
    assert_eq!(results.len(), 1);
    assert_eq!(results.get(0).unwrap(), (link_id.clone(), false, 0, Some(10)));
}

#[test]
fn test_verify_batch_empty_input_returns_empty_vec() {
    let env = Env::default();
    env.mock_all_auths();
    let (_merchant, client) = setup_payment_link(&env);

    let results = client.verify_batch(&soroban_sdk::vec![&env]);
    assert!(results.is_empty());
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_max_uses() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "limited_link");
    client.create_link(
        &merchant,
        &link_id,
        &None,
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Limit"),
        &None,
        &Some(1),
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    client.use_link(&payer, &link_id, &100i128, &None);
    // Should fail on second use
    client.use_link(&payer, &link_id, &100i128, &None);
}

// -- Issue #111: Direct-to-Merchant Payment Flow ------------------------------

#[test]
fn test_direct_transfer_link_transfers_to_merchant() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let usdc_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &usdc_token);

    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    // Fund payer
    token_admin_client.mint(&payer, &5000i128);

    let link_id = String::from_str(&env, "direct_link");
    let amount = 1000i128;
    client.create_link(
        &merchant,
        &link_id,
        &Some(amount),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Direct"),
        &None,
        &None,
        &true,
        &None,
        &MaybeFiatConfig::None,
    );

    let link = client.get_link(&link_id);
    assert!(link.direct_transfer);

    let token_client = token::TokenClient::new(&env, &usdc_token);
    let merchant_balance_before = token_client.balance(&merchant);

    client.use_link(&payer, &link_id, &amount, &Some(usdc_token.clone()));

    let merchant_balance_after = token_client.balance(&merchant);
    assert_eq!(merchant_balance_after - merchant_balance_before, amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_direct_transfer_without_token_address_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "direct_no_token");
    client.create_link(
        &merchant,
        &link_id,
        &Some(500i128),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Direct no token"),
        &None,
        &None,
        &true,
        &None,
        &MaybeFiatConfig::None,
    );

    // Should fail because usdc_token is None but direct_transfer is true
    client.use_link(&payer, &link_id, &500i128, &None);
}

// -- Issue #317: Payment Link Metadata Validation ----------------------------

#[test]
#[should_panic(expected = "Error(Contract, #49)")]
fn test_metadata_too_large_21_keys() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);

    let link_id = String::from_str(&env, "meta_large");
    let keys_21 = [
        "k0","k1","k2","k3","k4","k5","k6","k7","k8","k9",
        "k10","k11","k12","k13","k14","k15","k16","k17","k18","k19","k20",
    ];
    let mut metadata = Map::new(&env);
    for k in keys_21.iter() {
        metadata.set(String::from_str(&env, k), String::from_str(&env, "v"));
    }

    client.create_link(
        &merchant,
        &link_id,
        &None,
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Meta Test"),
        &None,
        &None,
        &false,
        &Some(metadata),
        &MaybeFiatConfig::None,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #47)")]
fn test_metadata_value_too_long_257_chars() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);

    let link_id = String::from_str(&env, "meta_long");
    let mut metadata = Map::new(&env);
    let long_value = String::from_str(&env, "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
    metadata.set(String::from_str(&env, "key"), long_value);

    client.create_link(
        &merchant,
        &link_id,
        &None,
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Meta Test"),
        &None,
        &None,
        &false,
        &Some(metadata),
        &MaybeFiatConfig::None,
    );
}

#[test]
fn test_metadata_20_keys_256_char_values_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);

    let link_id = String::from_str(&env, "meta_valid");
    let mut metadata = Map::new(&env);
    let keys_20 = [
        "k0","k1","k2","k3","k4","k5","k6","k7","k8","k9",
        "k10","k11","k12","k13","k14","k15","k16","k17","k18","k19",
    ];
    let val256 = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    for k in keys_20.iter() {
        metadata.set(String::from_str(&env, k), String::from_str(&env, val256));
    }

    let id = client.create_link(
        &merchant,
        &link_id,
        &None,
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Meta Test"),
        &None,
        &None,
        &false,
        &Some(metadata),
        &MaybeFiatConfig::None,
    );

    assert_eq!(id, link_id);
    let link = client.get_link(&link_id);
    assert!(link.metadata.is_some());
}

#[test]
fn test_metadata_none_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);

    let link_id = String::from_str(&env, "meta_none");
    let id = client.create_link(
        &merchant,
        &link_id,
        &None,
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Meta Test"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    assert_eq!(id, link_id);
    let link = client.get_link(&link_id);
    assert!(link.metadata.is_none());
}

// -- Issue #413: Multi-Currency Invoicing (Fiat) ----------------------------

#[test]
fn test_create_fiat_link_and_use_with_rate() {
    let env = Env::default();
    env.mock_all_auths();

    // Deploy FX oracle
    let oracle_id = env.register(FXOracle, ());
    let oracle_client = FXOracleClient::new(&env, &oracle_id);
    let oracle_admin = Address::generate(&env);
    oracle_client.oracle_initialize(&oracle_admin, &86400);
    let oracle = Address::generate(&env);
    oracle_client.oracle_grant_role(&oracle_admin, &Symbol::new(&env, "ORACLE"), &oracle);

    // Set rate: 1.0 USD per USDC (rate = 1_0000000, 7 decimals)
    oracle_client.set_rate(&oracle, &Symbol::new(&env, "USD"), &1_0000000i128, &7);

    // Deploy payment link manager
    let (merchant, client) = setup_payment_link(&env);

    let link_id = String::from_str(&env, "fiat_link");
    let fiat = FiatConfig {
        amount: 100i128,
        currency: Symbol::new(&env, "USD"),
        oracle: oracle_id.clone(),
    };

    let id = client.create_link(
        &merchant,
        &link_id,
        &None, // amount: open (allow any USDC)
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Fiat Invoice"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::Some(fiat),
    );

    assert_eq!(id, link_id);
    let link = client.get_link(&link_id);
    let stored_fiat = link.fiat.into_option().unwrap();
    assert_eq!(stored_fiat.amount, 100);
    assert_eq!(stored_fiat.currency, Symbol::new(&env, "USD"));
    assert_eq!(stored_fiat.oracle, oracle_id);
}

#[test]
fn test_use_fiat_link_requires_correct_usdc() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle_id = env.register(FXOracle, ());
    let oracle_client = FXOracleClient::new(&env, &oracle_id);
    let oracle_admin = Address::generate(&env);
    oracle_client.oracle_initialize(&oracle_admin, &86400);
    let oracle = Address::generate(&env);
    oracle_client.oracle_grant_role(&oracle_admin, &Symbol::new(&env, "ORACLE"), &oracle);

    // Rate: 1 USD = 2 USDC
    oracle_client.set_rate(&oracle, &Symbol::new(&env, "USD"), &2_0000000i128, &7);

    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "fiat_use");
    let fiat = FiatConfig {
        amount: 50i128, // $50 ? should require 25 USDC (50/2)
        currency: Symbol::new(&env, "USD"),
        oracle: oracle_id,
    };

    client.create_link(
        &merchant,
        &link_id,
        &None, // amount: open
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Fiat Use"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::Some(fiat),
    );

    // Should succeed with correct USDC equivalent (50 * 10^7 / 2_0000000 = 25)
    let payment_id = client.use_link(&payer, &link_id, &25i128, &None);
    assert!(!payment_id.is_empty());
    let link = client.get_link(&link_id);
    assert_eq!(link.use_count, 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #406)")]
fn test_use_fiat_link_rejects_wrong_usdc() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle_id = env.register(FXOracle, ());
    let oracle_client = FXOracleClient::new(&env, &oracle_id);
    let oracle_admin = Address::generate(&env);
    oracle_client.oracle_initialize(&oracle_admin, &86400);
    let oracle = Address::generate(&env);
    oracle_client.oracle_grant_role(&oracle_admin, &Symbol::new(&env, "ORACLE"), &oracle);

    oracle_client.set_rate(&oracle, &Symbol::new(&env, "USD"), &1_0000000i128, &7);

    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "fiat_wrong");
    let fiat = FiatConfig {
        amount: 100i128, // $100 ? should require 100 USDC (rate 1.0)
        currency: Symbol::new(&env, "USD"),
        oracle: oracle_id,
    };

    client.create_link(
        &merchant,
        &link_id,
        &None,
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Fiat Wrong"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::Some(fiat),
    );

    // 50 USDC is wrong when fiat_amount=100 at rate=1
    client.use_link(&payer, &link_id, &50i128, &None);
}

// ── Payment Link Analytics (view_count, total_revenue, conversion_rate) ─────

#[test]
fn test_record_link_view_increments_view_count() {
    let env = Env::default();
    env.mock_all_auths();
    let (_merchant, client) = setup_payment_link(&env);

    let link_id = String::from_str(&env, "view_link");
    client.create_link(
        &_merchant,
        &link_id,
        &None,
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "View Test"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    // Initially zero views
    let link = client.get_link(&link_id);
    assert_eq!(link.view_count, 0);

    // Record 3 views
    client.record_link_view(&link_id);
    client.record_link_view(&link_id);
    client.record_link_view(&link_id);

    let link = client.get_link(&link_id);
    assert_eq!(link.view_count, 3);
}

#[test]
fn test_use_link_accumulates_revenue() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "revenue_link");
    let amount = 1000i128;
    client.create_link(
        &merchant,
        &link_id,
        &Some(amount),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Revenue"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    // Initially zero revenue
    let link = client.get_link(&link_id);
    assert_eq!(link.total_revenue, 0);

    // First use
    client.use_link(&payer, &link_id, &amount, &None);
    let link = client.get_link(&link_id);
    assert_eq!(link.total_revenue, amount);

    // Second use (different payer)
    let payer2 = Address::generate(&env);
    client.use_link(&payer2, &link_id, &amount, &None);
    let link = client.get_link(&link_id);
    assert_eq!(link.total_revenue, amount * 2);
}

#[test]
fn test_get_link_analytics_conversion_rate() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "analytics_link");
    let amount = 1000i128;
    client.create_link(
        &merchant,
        &link_id,
        &Some(amount),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Analytics"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    // Record 10 views
    for _ in 0..10 {
        client.record_link_view(&link_id);
    }

    // Use the link once (1 conversion out of 10 views = 10%)
    client.use_link(&payer, &link_id, &amount, &None);

    let analytics = client.get_link_analytics(&link_id);
    assert_eq!(analytics.view_count, 10);
    assert_eq!(analytics.use_count, 1);
    assert_eq!(analytics.total_revenue, amount);
    // 1/10 = 10% = 1000 bps
    assert_eq!(analytics.conversion_rate, 1000);
}

#[test]
fn test_get_link_analytics_zero_views() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "zero_views_link");
    let amount = 500i128;
    client.create_link(
        &merchant,
        &link_id,
        &Some(amount),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Zero Views"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    // Use the link without any views
    client.use_link(&payer, &link_id, &amount, &None);

    let analytics = client.get_link_analytics(&link_id);
    assert_eq!(analytics.view_count, 0);
    assert_eq!(analytics.use_count, 1);
    assert_eq!(analytics.total_revenue, amount);
    // No views → conversion rate is 0 (avoid division by zero)
    assert_eq!(analytics.conversion_rate, 0);
}

#[test]
fn test_get_link_analytics_full_conversion() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);
    let payer = Address::generate(&env);

    let link_id = String::from_str(&env, "full_conv_link");
    let amount = 1000i128;
    client.create_link(
        &merchant,
        &link_id,
        &Some(amount),
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Full Conversion"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    // 4 views, 4 uses → 100% conversion = 10000 bps
    for _ in 0..4 {
        client.record_link_view(&link_id);
        let p = Address::generate(&env);
        client.use_link(&p, &link_id, &amount, &None);
    }

    let analytics = client.get_link_analytics(&link_id);
    assert_eq!(analytics.view_count, 4);
    assert_eq!(analytics.use_count, 4);
    assert_eq!(analytics.total_revenue, amount * 4);
    assert_eq!(analytics.conversion_rate, 10000);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_record_link_view_rejects_inactive_link() {
    let env = Env::default();
    env.mock_all_auths();
    let (merchant, client) = setup_payment_link(&env);

    let link_id = String::from_str(&env, "inactive_view_link");
    client.create_link(
        &merchant,
        &link_id,
        &None,
        &Symbol::new(&env, "USDC"),
        &String::from_str(&env, "Inactive View"),
        &None,
        &None,
        &false,
        &None,
        &MaybeFiatConfig::None,
    );

    client.deactivate_link(&merchant, &link_id);
    // Should fail because the link is no longer active
    client.record_link_view(&link_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #404)")]
fn test_get_link_analytics_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (_merchant, client) = setup_payment_link(&env);

    let nonexistent = String::from_str(&env, "does_not_exist");
    client.get_link_analytics(&nonexistent);
}



